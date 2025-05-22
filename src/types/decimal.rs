use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    str::FromStr,
};
use crate::errors::Error;

static FACTORS10: &[i64] = &[
    1,
    10,
    100,
    1000,
    10000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
    1_000_000_000_000_000,
    10_000_000_000_000_000,
    100_000_000_000_000_000,
    1_000_000_000_000_000_000,
];

pub trait Base {
    fn scale(self, scale: i64) -> i64;
}

pub trait InternalResult {
    fn get(underlying: i64) -> Self;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum NoBits {
    N32,
    N64,
}

/// Provides arbitrary-precision floating point decimal.
#[derive(Clone)]
pub struct Decimal {
    pub(crate) underlying: i64,
    pub(crate) nobits: NoBits, // its domain is {32, 64}
    pub(crate) precision: u8,
    pub(crate) scale: u8,
}

impl Default for Decimal {
    fn default() -> Self {
        Decimal {
            underlying: 0,
            precision: 9,
            scale: 4,
            nobits: NoBits::N32,
        }
    }
}

impl Hash for Decimal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.underlying.hash(state);
        self.nobits.hash(state);
        self.precision.hash(state);
        self.scale.hash(state);
    }
}

macro_rules! base_for {
    ( $( $t:ty: $cast:expr ),* ) => {
        $(
            impl Base for $t {
                fn scale(self, scale: i64) -> i64 {
                    $cast(self * (scale as $t)) as i64
                }
            }
        )*
    };
}

base_for! {
    f32: std::convert::identity,
    f64: std::convert::identity,
    i8: i64::from,
    i16: i64::from,
    i32: i64::from,
    i64: std::convert::identity,
    u8: i64::from,
    u16: i64::from,
    u32: i64::from,
    u64 : std::convert::identity
}

impl InternalResult for i32 {
    #[inline(always)]
    fn get(underlying: i64) -> Self {
        underlying as Self
    }
}

impl InternalResult for i64 {
    #[inline(always)]
    fn get(underlying: i64) -> Self {
        underlying
    }
}

impl NoBits {
    pub(crate) fn from_precision(precision: u8) -> Option<NoBits> {
        if precision <= 9 {
            Some(NoBits::N32)
        } else if precision <= 18 {
            Some(NoBits::N64)
        } else {
            None
        }
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        match self.scale.cmp(&other.scale) {
            Ordering::Less => {
                let delta = other.scale() - self.scale();
                let underlying = self.underlying * FACTORS10[delta];
                other.underlying == underlying
            }
            Ordering::Equal => self.underlying == other.underlying,
            Ordering::Greater => {
                let delta = self.scale() - other.scale();
                let underlying = other.underlying * FACTORS10[delta];
                self.underlying == underlying
            }
        }
    }
}

fn decimal2str(decimal: &Decimal) -> String {
    let mut r = format!("{}", decimal.underlying);
    while r.len() < decimal.scale() {
        r.insert(0, '0');
    }
    let pos = r.len() - decimal.scale();
    r.insert(pos, '.');
    if r.starts_with('.') {
        r.insert(0, '0');
    }
    r
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", decimal2str(self))
    }
}

impl fmt::Debug for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", decimal2str(self))
    }
}

impl From<Decimal> for f32 {
    fn from(value: Decimal) -> Self {
        value.underlying as f32 / FACTORS10[value.scale()] as f32
    }
}

impl From<Decimal> for f64 {
    fn from(value: Decimal) -> Self {
        value.underlying as f64 / FACTORS10[value.scale()] as f64
    }
}

impl Decimal {
    /// Method of creating a Decimal.
    pub fn new(underlying: i64, scale: u8) -> Decimal {
        let precision = 18;
        if scale > precision {
            panic!("scale can't be greater than 18");
        }

        Decimal {
            underlying,
            precision,
            scale,
            nobits: NoBits::N64,
        }
    }

    pub fn of<B: Base>(source: B, scale: u8) -> Decimal {
        let precision = 18;
        if scale > precision {
            panic!("scale can't be greater than 18");
        }

        let underlying = source.scale(FACTORS10[scale as usize]);
        if underlying > FACTORS10[precision as usize] {
            panic!("{} > {}", underlying, FACTORS10[precision as usize]);
        }

        Decimal {
            underlying,
            precision,
            scale,
            nobits: NoBits::N64,
        }
    }

    /// Get the internal representation of decimal as [`i32`] or [`i64`].
    ///
    /// example:
    /// ```rust
    /// # use std::env;
    /// # use clickhouse_rs::{Pool, types::Decimal, errors::Result};
    /// # let mut rt = tokio::runtime::Runtime::new().unwrap();
    /// # let ret: Result<()> = rt.block_on(async {
    /// #     let database_url = env::var("DATABASE_URL")
    /// #         .unwrap_or("tcp://localhost:9000?compression=lz4".into());
    /// #     let pool = Pool::new(database_url);
    ///       let mut c = pool.get_handle().await?;
    ///       let block = c.query("SELECT toDecimal32(2, 4) AS x").fetch_all().await?;
    ///
    ///       let x: Decimal = block.get(0, "x")?;
    ///       let actual: i32 = x.internal();
    ///       assert_eq!(20000, actual);
    /// #     Ok(())
    /// # });
    /// # ret.unwrap()
    /// ```
    #[inline(always)]
    pub fn internal<I: InternalResult>(&self) -> I {
        InternalResult::get(self.underlying)
    }

    /// Determines how many decimal digits fraction can have.
    pub fn scale(&self) -> usize {
        self.scale as usize
    }

    pub(crate) fn set_scale(self, scale: u8) -> Self {
        let underlying = match scale.cmp(&self.scale) {
            Ordering::Less => {
                let delta = self.scale() - scale as usize;
                self.underlying / FACTORS10[delta]
            }
            Ordering::Equal => return self,
            Ordering::Greater => {
                let delta = scale as usize - self.scale();
                self.underlying * FACTORS10[delta]
            }
        };

        Decimal {
            underlying,
            precision: self.precision,
            scale,
            nobits: self.nobits,
        }
    }
}

impl FromStr for Decimal {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // Handle empty string
        if s.is_empty() {
            return Err(Error::Other("Empty string cannot be converted to Decimal".into()));
        }

        // Check for negative sign
        let (is_negative, s) = if s.starts_with('-') {
            (true, &s[1..])
        } else {
            (false, s)
        };

        // Split by decimal point
        let parts: Vec<&str> = s.split('.').collect();

        if parts.len() > 2 {
            return Err(Error::Other("Multiple decimal points".into()));
        }

        let (integer_part, fractional_part) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            (parts[0], "")
        };

        // Validate that all characters are digits
        if !integer_part.chars().all(|c| c.is_ascii_digit()) {
            return Err(Error::Other("Invalid integer part in Decimal string".into()));
        }

        if !fractional_part.chars().all(|c| c.is_ascii_digit()) {
            return Err(Error::Other("Invalid fractional part in Decimal string".into()));
        }

        // Handle the case where integer part is empty (e.g., ".123")
        let integer_part = if integer_part.is_empty() { "0" } else { integer_part };

        // Derive scale from the length of fractional part
        let scale = fractional_part.len() as u8;

        // Combine integer and fractional parts
        let combined = format!("{}{}", integer_part, fractional_part);

        // Parse to i64
        let underlying = match combined.parse::<i64>() {
            Ok(val) => if is_negative { -val } else { val },
            Err(_) => return Err(Error::Other("Decimal number too large or invalid".into())),
        };

        // Calculate precision (total number of significant digits)
        let precision = combined.len() as u8;

        Ok(Decimal {
            underlying,
            precision,
            scale,
            nobits: NoBits::N64,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new() {
        assert_eq!(Decimal::new(2, 1), Decimal::of(0.2_f64, 1));
        assert_eq!(Decimal::new(2, 5), Decimal::of(0.00002_f64, 5));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Decimal::of(2.1_f32, 4)), "2.1000");
        assert_eq!(format!("{}", Decimal::of(0.2_f64, 4)), "0.2000");
        assert_eq!(format!("{}", Decimal::of(2, 4)), "2.0000");
        assert_eq!(format!("{:?}", Decimal::of(2, 4)), "2.0000");
    }

    #[test]
    fn test_eq() {
        assert_eq!(Decimal::of(2.0_f64, 4), Decimal::of(2.0_f64, 4));
        assert_ne!(Decimal::of(3.0_f64, 4), Decimal::of(2.0_f64, 4));

        assert_eq!(Decimal::of(2.0_f64, 4), Decimal::of(2.0_f64, 2));
        assert_ne!(Decimal::of(2.0_f64, 4), Decimal::of(3.0_f64, 2));

        assert_eq!(Decimal::of(2.0_f64, 2), Decimal::of(2.0_f64, 4));
        assert_ne!(Decimal::of(3.0_f64, 2), Decimal::of(2.0_f64, 4));
    }

    #[test]
    fn test_internal32() {
        let internal: i32 = Decimal::of(2, 4).internal();
        assert_eq!(internal, 20000_i32);
    }

    #[test]
    fn test_internal64() {
        let internal: i64 = Decimal::of(2, 4).internal();
        assert_eq!(internal, 20000_i64);
    }

    #[test]
    fn test_scale() {
        assert_eq!(Decimal::of(2, 4).scale(), 4);
    }

    #[test]
    fn test_from_f32() {
        let value: f32 = Decimal::of(2, 4).into();
        assert!((value - 2.0_f32).abs() <= std::f32::EPSILON);
    }

    #[test]
    fn test_from_f64() {
        let value: f64 = Decimal::of(2, 4).into();
        assert!((value - 2.0_f64).abs() < std::f64::EPSILON);
    }

    #[test]
    fn set_scale1() {
        let a = Decimal::of(12, 3);
        let b = a.set_scale(2);

        assert_eq!(2, b.scale);
        assert_eq!(1200, b.underlying);
    }

    #[test]
    fn set_scale2() {
        let a = Decimal::of(12, 3);
        let b = a.set_scale(4);

        assert_eq!(4, b.scale);
        assert_eq!(120_000, b.underlying);
    }

    #[test]
    fn test_decimal2str() {
        let d = Decimal::of(0.00001, 5);
        let actual = decimal2str(&d);
        assert_eq!(actual, "0.00001".to_string());
    }

    #[test]
    fn test_str2decimal_basic() {
        let decimal = Decimal::from_str("123.45").unwrap();
        assert_eq!(decimal.underlying, 12345);
        assert_eq!(decimal.precision, 5); // 5 digits total
        assert_eq!(decimal.scale, 2);     // 2 decimal places

        // Test round trip
        let str_result = decimal2str(&decimal);
        assert_eq!(str_result, "123.45");
    }

    #[test]
    fn test_str2decimal_no_fractional() {
        let decimal = Decimal::from_str("123").unwrap();
        assert_eq!(decimal.underlying, 123);
        assert_eq!(decimal.precision, 3); // 3 digits total
        assert_eq!(decimal.scale, 0);     // 0 decimal places

        let str_result = decimal2str(&decimal);
        assert_eq!(str_result, "123.");
    }

    #[test]
    fn test_str2decimal_leading_dot() {
        let decimal = Decimal::from_str(".123").unwrap();
        assert_eq!(decimal.underlying, 123);
        assert_eq!(decimal.precision, 4); // "0123" = 4 digits
        assert_eq!(decimal.scale, 3);     // 3 decimal places

        let str_result = decimal2str(&decimal);
        assert_eq!(str_result, "0.123");
    }

    #[test]
    fn test_str2decimal_negative() {
        let decimal = Decimal::from_str("-123.45").unwrap();
        assert_eq!(decimal.underlying, -12345);
        assert_eq!(decimal.precision, 5); // 5 digits total (sign doesn't count)
        assert_eq!(decimal.scale, 2);     // 2 decimal places

        let str_result = decimal2str(&decimal);
        assert_eq!(str_result, "-123.45");
    }

    #[test]
    fn test_str2decimal_zero() {
        let decimal = Decimal::from_str("0").unwrap();
        assert_eq!(decimal.underlying, 0);
        assert_eq!(decimal.precision, 1);
        assert_eq!(decimal.scale, 0);

        let str_result = decimal2str(&decimal);
        assert_eq!(str_result, "0.");
    }

    #[test]
    fn test_str2decimal_zero_fractional() {
        let decimal = Decimal::from_str("0.00").unwrap();
        assert_eq!(decimal.underlying, 0);
        assert_eq!(decimal.precision, 3); // "000" = 3 digits
        assert_eq!(decimal.scale, 2);     // 2 decimal places

        let str_result = decimal2str(&decimal);
        assert_eq!(str_result, "0.00");
    }

    #[test]
    fn test_str2decimal_leading_zeros() {
        let decimal = Decimal::from_str("007.120").unwrap();
        assert_eq!(decimal.underlying, 7120);
        assert_eq!(decimal.precision, 6); // "007120" = 6 digits
        assert_eq!(decimal.scale, 3);     // 3 decimal places

        let str_result = decimal2str(&decimal);
        assert_eq!(str_result, "7.120");
    }

    #[test]
    fn test_str2decimal_errors() {
        assert!(Decimal::from_str("").is_err());
        assert!(Decimal::from_str("12.34.56").is_err());
        assert!(Decimal::from_str("abc").is_err());
        assert!(Decimal::from_str("12.3abc").is_err());
    }
}

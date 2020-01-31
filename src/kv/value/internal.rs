use std::fmt;

use super::{Error, Fill, Slot};
use kv;

// `Visitor` is an internal API for visiting the structure of a value.
// It's not intended to be public (at this stage).

/// A container for a structured value for a specific kind of visitor.
#[derive(Clone, Copy)]
pub(super) enum Inner<'v> {
    /// A simple primitive value that can be copied without allocating.
    Primitive(Primitive<'v>),
    /// A value that can be filled.
    Fill(&'v dyn Fill),
    /// A debuggable value.
    Debug(&'v dyn fmt::Debug),
    /// A displayable value.
    Display(&'v dyn fmt::Display),

    #[cfg(feature = "kv_unstable_sval")]
    /// A structured value from `sval`.
    Sval(&'v dyn sval_support::Value),
}

impl<'v> Inner<'v> {
    pub(super) fn visit(&self, visitor: &mut dyn Visitor) -> Result<(), Error> {
        match *self {
            Inner::Primitive(value) => match value {
                Primitive::Signed(value) => visitor.i64(value),
                Primitive::Unsigned(value) => visitor.u64(value),
                Primitive::Float(value) => visitor.f64(value),
                Primitive::Bool(value) => visitor.bool(value),
                Primitive::Char(value) => visitor.char(value),
                Primitive::Str(value) => visitor.str(value),
                Primitive::None => visitor.none(),
            },
            Inner::Fill(value) => value.fill(&mut Slot::new(visitor)),
            Inner::Debug(value) => visitor.debug(value),
            Inner::Display(value) => visitor.display(value),

            #[cfg(feature = "kv_unstable_sval")]
            Inner::Sval(value) => visitor.sval(value),
        }
    }
}

/// The internal serialization contract.
pub(super) trait Visitor {
    fn debug(&mut self, v: &dyn fmt::Debug) -> Result<(), Error>;
    fn display(&mut self, v: &dyn fmt::Display) -> Result<(), Error> {
        self.debug(&format_args!("{}", v))
    }

    fn u64(&mut self, v: u64) -> Result<(), Error>;
    fn i64(&mut self, v: i64) -> Result<(), Error>;
    fn f64(&mut self, v: f64) -> Result<(), Error>;
    fn bool(&mut self, v: bool) -> Result<(), Error>;
    fn char(&mut self, v: char) -> Result<(), Error>;
    fn str(&mut self, v: &str) -> Result<(), Error>;
    fn none(&mut self) -> Result<(), Error>;

    #[cfg(feature = "kv_unstable_sval")]
    fn sval(&mut self, v: &dyn sval_support::Value) -> Result<(), Error>;
}

#[derive(Clone, Copy)]
pub(super) enum Primitive<'v> {
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(&'v str),
    None,
}

mod coerce {
    use super::*;

    impl<'v> Inner<'v> {
        pub(in crate::kv::value) fn as_str(&self) -> Option<&str> {
            if let Inner::Primitive(Primitive::Str(value)) = self {
                Some(value)
            } else {
                self.coerce().into_primitive().into_str()
            }
        }

        pub(in crate::kv::value) fn as_u64(&self) -> Option<u64> {
            self.coerce().into_primitive().into_u64()
        }

        pub(in crate::kv::value) fn as_i64(&self) -> Option<i64> {
            self.coerce().into_primitive().into_i64()
        }

        pub(in crate::kv::value) fn as_f64(&self) -> Option<f64> {
            self.coerce().into_primitive().into_f64()
        }

        pub(in crate::kv::value) fn as_char(&self) -> Option<char> {
            self.coerce().into_primitive().into_char()
        }

        pub(in crate::kv::value) fn as_bool(&self) -> Option<bool> {
            self.coerce().into_primitive().into_bool()
        }

        fn coerce(&self) -> Coerced {
            struct Coerce<'v>(Coerced<'v>);

            impl<'v> Coerce<'v> {
                fn new() -> Self {
                    Coerce(Coerced::Primitive(Primitive::None))
                }
            }

            impl<'v> Visitor for Coerce<'v> {
                fn debug(&mut self, _: &dyn fmt::Debug) -> Result<(), Error> {
                    Ok(())
                }

                fn u64(&mut self, v: u64) -> Result<(), Error> {
                    self.0 = Coerced::Primitive(Primitive::Unsigned(v));
                    Ok(())
                }

                fn i64(&mut self, v: i64) -> Result<(), Error> {
                    self.0 = Coerced::Primitive(Primitive::Signed(v));
                    Ok(())
                }

                fn f64(&mut self, v: f64) -> Result<(), Error> {
                    self.0 = Coerced::Primitive(Primitive::Float(v));
                    Ok(())
                }

                fn bool(&mut self, v: bool) -> Result<(), Error> {
                    self.0 = Coerced::Primitive(Primitive::Bool(v));
                    Ok(())
                }

                fn char(&mut self, v: char) -> Result<(), Error> {
                    self.0 = Coerced::Primitive(Primitive::Char(v));
                    Ok(())
                }

                #[cfg(not(feature = "std"))]
                fn str(&mut self, v: &str) -> Result<(), Error> {
                    Ok(())
                }

                #[cfg(feature = "std")]
                fn str(&mut self, v: &str) -> Result<(), Error> {
                    self.0 = Coerced::String(v.into());
                    Ok(())
                }

                fn none(&mut self) -> Result<(), Error> {
                    self.0 = Coerced::Primitive(Primitive::None);
                    Ok(())
                }

                #[cfg(feature = "kv_unstable_sval")]
                fn sval(&mut self, v: &dyn sval_support::Value) -> Result<(), Error> {
                    self.0 = sval_support::coerce(v);
                    Ok(())
                }
            }

            let mut coerce = Coerce::new();
            let _ = self.visit(&mut coerce);
            coerce.0
        }
    }

    pub(super) enum Coerced<'v> {
        Primitive(Primitive<'v>),
        #[cfg(feature = "std")]
        String(String),
    }

    impl<'v> Coerced<'v> {
        fn into_primitive(self) -> Primitive<'v> {
            match self {
                Coerced::Primitive(value) => value,
                _ => Primitive::None,
            }
        }
    }

    impl<'v> Primitive<'v> {
        fn into_str(self) -> Option<&'v str> {
            if let Primitive::Str(value) = self {
                Some(value)
            } else {
                None
            }
        }

        fn into_u64(self) -> Option<u64> {
            if let Primitive::Unsigned(value) = self {
                Some(value)
            } else {
                None
            }
        }

        fn into_i64(self) -> Option<i64> {
            if let Primitive::Signed(value) = self {
                Some(value)
            } else {
                None
            }
        }

        fn into_f64(self) -> Option<f64> {
            if let Primitive::Float(value) = self {
                Some(value)
            } else {
                None
            }
        }

        fn into_char(self) -> Option<char> {
            if let Primitive::Char(value) = self {
                Some(value)
            } else {
                None
            }
        }

        fn into_bool(self) -> Option<bool> {
            if let Primitive::Bool(value) = self {
                Some(value)
            } else {
                None
            }
        }
    }

    #[cfg(feature = "std")]
    mod std_support {
        use super::*;

        use std::borrow::Cow;

        impl<'v> Inner<'v> {
            pub(in crate::kv::value) fn to_str(&self) -> Option<Cow<str>> {
                self.coerce().into_string()
            }
        }

        impl<'v> Coerced<'v> {
            pub(super) fn into_string(self) -> Option<Cow<'v, str>> {
                match self {
                    Coerced::Primitive(Primitive::Str(value)) => Some(value.into()),
                    Coerced::String(value) => Some(value.into()),
                    _ => None,
                }
            }
        }
    }
}

mod fmt_support {
    use super::*;

    impl<'v> kv::Value<'v> {
        /// Get a value from a debuggable type.
        pub fn from_debug<T>(value: &'v T) -> Self
        where
            T: fmt::Debug,
        {
            kv::Value {
                inner: Inner::Debug(value),
            }
        }

        /// Get a value from a displayable type.
        pub fn from_display<T>(value: &'v T) -> Self
        where
            T: fmt::Display,
        {
            kv::Value {
                inner: Inner::Display(value),
            }
        }
    }

    impl<'v> fmt::Debug for kv::Value<'v> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.visit(&mut FmtVisitor(f))?;

            Ok(())
        }
    }

    impl<'v> fmt::Display for kv::Value<'v> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.visit(&mut FmtVisitor(f))?;

            Ok(())
        }
    }

    struct FmtVisitor<'a, 'b: 'a>(&'a mut fmt::Formatter<'b>);

    impl<'a, 'b: 'a> Visitor for FmtVisitor<'a, 'b> {
        fn debug(&mut self, v: &dyn fmt::Debug) -> Result<(), Error> {
            v.fmt(self.0)?;

            Ok(())
        }

        fn u64(&mut self, v: u64) -> Result<(), Error> {
            self.debug(&format_args!("{:?}", v))
        }

        fn i64(&mut self, v: i64) -> Result<(), Error> {
            self.debug(&format_args!("{:?}", v))
        }

        fn f64(&mut self, v: f64) -> Result<(), Error> {
            self.debug(&format_args!("{:?}", v))
        }

        fn bool(&mut self, v: bool) -> Result<(), Error> {
            self.debug(&format_args!("{:?}", v))
        }

        fn char(&mut self, v: char) -> Result<(), Error> {
            self.debug(&format_args!("{:?}", v))
        }

        fn str(&mut self, v: &str) -> Result<(), Error> {
            self.debug(&format_args!("{:?}", v))
        }

        fn none(&mut self) -> Result<(), Error> {
            self.debug(&format_args!("None"))
        }

        #[cfg(feature = "kv_unstable_sval")]
        fn sval(&mut self, v: &dyn sval_support::Value) -> Result<(), Error> {
            sval_support::fmt(self.0, v)
        }
    }
}

#[cfg(feature = "kv_unstable_sval")]
pub(super) mod sval_support {
    use super::coerce::Coerced;
    use super::*;

    extern crate sval;

    impl<'v> kv::Value<'v> {
        /// Get a value from a structured type.
        pub fn from_sval<T>(value: &'v T) -> Self
        where
            T: sval::Value,
        {
            kv::Value {
                inner: Inner::Sval(value),
            }
        }
    }

    impl<'v> sval::Value for kv::Value<'v> {
        fn stream(&self, s: &mut sval::value::Stream) -> sval::value::Result {
            self.visit(&mut SvalVisitor(s)).map_err(Error::into_sval)?;

            Ok(())
        }
    }

    pub(in kv::value) use self::sval::Value;

    pub(super) fn fmt(f: &mut fmt::Formatter, v: &dyn sval::Value) -> Result<(), Error> {
        sval::fmt::debug(f, v)?;
        Ok(())
    }

    impl Error {
        fn from_sval(_: sval::value::Error) -> Self {
            Error::msg("`sval` serialization failed")
        }

        fn into_sval(self) -> sval::value::Error {
            sval::value::Error::msg("`sval` serialization failed")
        }
    }

    struct SvalVisitor<'a, 'b: 'a>(&'a mut sval::value::Stream<'b>);

    impl<'a, 'b: 'a> Visitor for SvalVisitor<'a, 'b> {
        fn debug(&mut self, v: &dyn fmt::Debug) -> Result<(), Error> {
            self.0
                .fmt(format_args!("{:?}", v))
                .map_err(Error::from_sval)
        }

        fn u64(&mut self, v: u64) -> Result<(), Error> {
            self.0.u64(v).map_err(Error::from_sval)
        }

        fn i64(&mut self, v: i64) -> Result<(), Error> {
            self.0.i64(v).map_err(Error::from_sval)
        }

        fn f64(&mut self, v: f64) -> Result<(), Error> {
            self.0.f64(v).map_err(Error::from_sval)
        }

        fn bool(&mut self, v: bool) -> Result<(), Error> {
            self.0.bool(v).map_err(Error::from_sval)
        }

        fn char(&mut self, v: char) -> Result<(), Error> {
            self.0.char(v).map_err(Error::from_sval)
        }

        fn str(&mut self, v: &str) -> Result<(), Error> {
            self.0.str(v).map_err(Error::from_sval)
        }

        fn none(&mut self) -> Result<(), Error> {
            self.0.none().map_err(Error::from_sval)
        }

        fn sval(&mut self, v: &dyn sval::Value) -> Result<(), Error> {
            self.0.any(v).map_err(Error::from_sval)
        }
    }

    pub(super) fn coerce<'v>(v: &dyn sval::Value) -> Coerced<'v> {
        struct Coerce<'v>(Coerced<'v>);

        impl<'v> sval::Stream for Coerce<'v> {
            fn u64(&mut self, v: u64) -> sval::stream::Result {
                self.0 = Coerced::Primitive(Primitive::Unsigned(v));
                Ok(())
            }

            fn i64(&mut self, v: i64) -> sval::stream::Result {
                self.0 = Coerced::Primitive(Primitive::Signed(v));
                Ok(())
            }

            fn f64(&mut self, v: f64) -> sval::stream::Result {
                self.0 = Coerced::Primitive(Primitive::Float(v));
                Ok(())
            }

            fn char(&mut self, v: char) -> sval::stream::Result {
                self.0 = Coerced::Primitive(Primitive::Char(v));
                Ok(())
            }

            fn bool(&mut self, v: bool) -> sval::stream::Result {
                self.0 = Coerced::Primitive(Primitive::Bool(v));
                Ok(())
            }

            #[cfg(feature = "std")]
            fn str(&mut self, s: &str) -> sval::stream::Result {
                self.0 = Coerced::String(s.into());
                Ok(())
            }
        }

        let mut coerce = Coerce(Coerced::Primitive(Primitive::None));
        let _ = sval::stream(&mut coerce, v);

        coerce.0
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use kv::value::test::Token;

        #[test]
        fn test_from_sval() {
            assert_eq!(kv::Value::from_sval(&42u64).to_token(), Token::Sval);
        }

        #[test]
        fn test_sval_structured() {
            let value = kv::Value::from(42u64);
            let expected = vec![sval::test::Token::Unsigned(42)];

            assert_eq!(sval::test::tokens(value), expected);
        }

        #[test]
        fn coersion() {
            assert_eq!(
                42u64,
                kv::Value::from_sval(&42u64)
                    .as_u64()
                    .expect("invalid value")
            );

            assert!(kv::Value::from_sval(&"a string").as_str().is_none());

            #[cfg(feature = "std")]
            assert_eq!(
                "a string",
                &*kv::Value::from_sval(&"a string")
                    .to_str()
                    .expect("invalid value")
            );
        }
    }
}

use std::fmt;
use std::vec::Vec;
use std::collections::HashMap;

#[derive(PartialEq)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    List(Vec<Value>),
    Dictionary(HashMap<String, Value>),
    Structure { signature: u8, fields: Vec<Value> },
    Message { signature: u8, fields: Vec<Value> },
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Value::Null => write!(f, "null"),
            Value::Boolean(ref value) => write!(f, "{:?}", value),
            Value::Integer(ref value) => write!(f, "{:?}", value),
            Value::Float(ref value) => write!(f, "{:?}", value),
            Value::String(ref value) => write!(f, "{:?}", value),
            Value::List(ref values) => write!(f, "{:?}", values),
            Value::Dictionary(ref values) => write!(f, "{:?}", values),
            Value::Structure { signature, ref fields } => write!(f, "#{:02X} {:?}", signature, fields),
            Value::Message { signature, ref fields } => write!(f, "Message<#{:02X}> {:?}", signature, fields),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Value::List(ref values) => write_tsv(f, values),
            _ => write!(f, "{:?}", self),
        }
    }
}

pub fn write_tsv(f: &mut fmt::Formatter, values: &[Value]) -> fmt::Result {
    let last = values.len() - 1;
    for value in values[..last].iter() {
        let _ = write!(f, "{:?}\t", value);
    }
    write!(f, "{:?}", values[last])
}

pub trait ValueCast {
    fn from(&self) -> Value;
}

macro_rules! impl_ValueCast_to_Integer {
    ($T:ty) => {
        impl ValueCast for $T {
            fn from(&self) -> Value {
               Value::Integer(*self as i64)
            }
        }
    }
}

macro_rules! impl_ValueCast_to_Float {
    ($T:ty) => {
        impl ValueCast for $T {
            fn from(&self) -> Value {
               Value::Float(*self as f64)
            }
        }
    }
}

macro_rules! impl_ValueCast_to_List {
    ($T:ty) => {
        impl ValueCast for $T {
            fn from(&self) -> Value {
               Value::List(self.iter().map(|&x| ValueCast::from(&x)).collect::<Vec<Value>>())
            }
        }
    }
}

impl ValueCast for bool {
    fn from(&self) -> Value {
        Value::Boolean(*self)
    }
}

impl ValueCast for char {
    fn from(&self) -> Value {
        let mut s = String::with_capacity(4);
        s.push(*self);
        Value::String(s)
    }
}

impl_ValueCast_to_Integer!(i8);
impl_ValueCast_to_Integer!(i16);
impl_ValueCast_to_Integer!(i32);
impl_ValueCast_to_Integer!(i64);
impl_ValueCast_to_Integer!(isize);

impl_ValueCast_to_Integer!(u8);
impl_ValueCast_to_Integer!(u16);
impl_ValueCast_to_Integer!(u32);
impl_ValueCast_to_Integer!(u64);
impl_ValueCast_to_Integer!(usize);

impl_ValueCast_to_Float!(f32);
impl_ValueCast_to_Float!(f64);

impl_ValueCast_to_List!([i8]);
impl_ValueCast_to_List!([i16]);
impl_ValueCast_to_List!([i32]);
impl_ValueCast_to_List!([i64]);
impl_ValueCast_to_List!([isize]);

impl_ValueCast_to_List!([u8]);
impl_ValueCast_to_List!([u16]);
impl_ValueCast_to_List!([u32]);
impl_ValueCast_to_List!([u64]);
impl_ValueCast_to_List!([usize]);

impl_ValueCast_to_List!(Vec<i8>);
impl_ValueCast_to_List!(Vec<i16>);
impl_ValueCast_to_List!(Vec<i32>);
impl_ValueCast_to_List!(Vec<i64>);
impl_ValueCast_to_List!(Vec<isize>);

impl_ValueCast_to_List!(Vec<u8>);
impl_ValueCast_to_List!(Vec<u16>);
impl_ValueCast_to_List!(Vec<u32>);
impl_ValueCast_to_List!(Vec<u64>);
impl_ValueCast_to_List!(Vec<usize>);
impl_ValueCast_to_List!(Vec<&str>);

impl ValueCast for Vec<String> {
    fn from(&self) -> Value {
        Value::List(self.iter().map(|x| ValueCast::from(x)).collect::<Vec<Value>>())
    }
}

impl<'t> ValueCast for &'t str {
    fn from(&self) -> Value {
        let mut s = String::with_capacity(self.len());
        s.push_str(&self);
        Value::String(s)
    }
}

impl ValueCast for String {
    fn from(&self) -> Value {
        let mut s = String::with_capacity(self.len());
        s.push_str(&self[..]);
        Value::String(s)
    }
}

pub trait ValueMatch {
    fn is_null(&self) -> bool;
    fn is_boolean(&self) -> bool;
    fn is_integer(&self) -> bool;
    fn is_float(&self) -> bool;
    fn is_string(&self) -> bool;
    fn is_list(&self) -> bool;
    fn is_map(&self) -> bool;
    fn is_structure(&self) -> bool;
}

impl ValueMatch for Value {

    fn is_null(&self) -> bool {
        match *self {
            Value::Null => true,
            _ => false
        }
    }

    fn is_boolean(&self) -> bool {
        match *self {
            Value::Boolean(_) => true,
            _ => false
        }
    }

    fn is_integer(&self) -> bool {
        match *self {
            Value::Integer(_) => true,
            _ => false
        }
    }

    fn is_float(&self) -> bool {
        match *self {
            Value::Float(_) => true,
            _ => false
        }
    }

    fn is_string(&self) -> bool {
        match *self {
            Value::String(_) => true,
            _ => false
        }
    }

    fn is_list(&self) -> bool {
        match *self {
            Value::List(_) => true,
            _ => false
        }
    }

    fn is_map(&self) -> bool {
        match *self {
            Value::Dictionary(_) => true,
            _ => false
        }
    }

    fn is_structure(&self) -> bool {
        match *self {
            Value::Structure { signature: _, fields: _ } => true,
            _ => false
        }
    }

}

pub enum Data {
    Record(Vec<Value>),
}
impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Data::Record(ref values) => write!(f, "Record({:?})", values),
        }
    }
}
impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Data::Record(ref data) => write_tsv(f, data),
        }
    }
}

#[macro_export]
macro_rules! parameters(
    {} => {
        {
            use std::collections::HashMap;

            HashMap::new()
        }
    };

    { $($key:expr => $value:expr),* } => {
        {
            use std::collections::HashMap;
            use $crate::values::{Value, ValueCast};

            let mut map : HashMap<&str, Value> = HashMap::new();
            $(
                map.insert($key, ValueCast::from(&$value));
            )+;

            map
        }
    };
);

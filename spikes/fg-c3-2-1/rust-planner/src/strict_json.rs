//! Bounded JSON parser used only at the isolated planner protocol boundary.
//! It rejects duplicate decoded object keys before serde_json::Value can erase
//! their presence and accounts for nesting and total values during parsing.

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::cell::Cell;
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrictJsonError {
    Invalid,
    DuplicateMember,
    DepthLimit,
    WorkLimit,
}

#[derive(Debug)]
struct Budget {
    max_depth: usize,
    values: Cell<usize>,
    max_values: usize,
}

#[derive(Clone)]
struct Seed {
    budget: Rc<Budget>,
    depth: usize,
}

struct JsonVisitor {
    budget: Rc<Budget>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for Seed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let count = self.budget.values.get().saturating_add(1);
        if count > self.budget.max_values {
            return Err(de::Error::custom("STRICT_JSON_WORK_LIMIT"));
        }
        self.budget.values.set(count);
        deserializer.deserialize_any(JsonVisitor {
            budget: self.budget,
            depth: self.depth,
        })
    }
}

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded JSON without duplicate object members")
    }

    fn visit_unit<E>(self) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Null)
    }
    fn visit_none<E>(self) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Null)
    }
    fn visit_bool<E>(self, value: bool) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E>(self, value: i64) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E>(self, value: u64) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E>(self, value: f64) -> Result<Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("STRICT_JSON_NON_FINITE"))
    }
    fn visit_str<E>(self, value: &str) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_owned()))
    }
    fn visit_string<E>(self, value: String) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let depth = self.depth.saturating_add(1);
        if depth > self.budget.max_depth {
            return Err(de::Error::custom("STRICT_JSON_DEPTH_LIMIT"));
        }
        let mut values =
            Vec::with_capacity(seq.size_hint().unwrap_or(0).min(self.budget.max_values));
        while let Some(value) = seq.next_element_seed(Seed {
            budget: self.budget.clone(),
            depth,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let depth = self.depth.saturating_add(1);
        if depth > self.budget.max_depth {
            return Err(de::Error::custom("STRICT_JSON_DEPTH_LIMIT"));
        }
        let mut values = Map::new();
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom("STRICT_JSON_DUPLICATE_MEMBER"));
            }
            let value = map.next_value_seed(Seed {
                budget: self.budget.clone(),
                depth,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

pub fn parse_bounded(
    raw: &str,
    max_depth: usize,
    max_values: usize,
) -> Result<Value, StrictJsonError> {
    let budget = Rc::new(Budget {
        max_depth,
        values: Cell::new(0),
        max_values,
    });
    let mut deserializer = serde_json::Deserializer::from_str(raw);
    let value = Seed { budget, depth: 0 }
        .deserialize(&mut deserializer)
        .map_err(|error| {
            let message = error.to_string();
            if message.contains("STRICT_JSON_DUPLICATE_MEMBER") {
                StrictJsonError::DuplicateMember
            } else if message.contains("STRICT_JSON_DEPTH_LIMIT") {
                StrictJsonError::DepthLimit
            } else if message.contains("STRICT_JSON_WORK_LIMIT") {
                StrictJsonError::WorkLimit
            } else {
                StrictJsonError::Invalid
            }
        })?;
    deserializer.end().map_err(|_| StrictJsonError::Invalid)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_literal_and_decoded_duplicate_members_at_any_depth() {
        assert_eq!(
            parse_bounded(r#"{"a":1,"a":2}"#, 64, 100),
            Err(StrictJsonError::DuplicateMember)
        );
        assert_eq!(
            parse_bounded(r#"{"a":{"name":1,"na\u006de":2}}"#, 64, 100),
            Err(StrictJsonError::DuplicateMember)
        );
    }

    #[test]
    fn bounds_container_depth_and_value_count_at_parse_time() {
        assert_eq!(
            parse_bounded(r#"{"a":{"b":1}}"#, 1, 100),
            Err(StrictJsonError::DepthLimit)
        );
        assert_eq!(
            parse_bounded(r#"[1,2,3]"#, 64, 3),
            Err(StrictJsonError::WorkLimit)
        );
        assert_eq!(
            parse_bounded(r#"[1,2]"#, 64, 3).unwrap(),
            serde_json::json!([1, 2])
        );
    }
}

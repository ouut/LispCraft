//! # Helper parsers and converters for SteelVal and Macroquad / lc_render types

use lc_core::EntityDesc;
use macroquad::color::Color;
use macroquad::math::{vec2, vec3, Vec2, Vec3};
use serde_json::{Map, Number, Value};
use steel::gc::Gc;
use steel::rvals::SteelVal;
use steel::HashMap;

/// Convert a Scheme [`SteelVal`] reference to a [`serde_json::Value`].
pub fn steel_val_to_json(val: &SteelVal) -> Result<Value, String> {
    match val {
        SteelVal::BoolV(b) => Ok(Value::Bool(*b)),
        SteelVal::NumV(n) => {
            Number::from_f64(*n)
                .map(Value::Number)
                .ok_or_else(|| format!("Non-finite f64: {}", n))
        }
        SteelVal::IntV(i) => Ok(Value::Number((*i as i64).into())),
        SteelVal::Rational(r) => {
            let f = (*r.numer() as f64) / (*r.denom() as f64);
            Number::from_f64(f)
                .map(Value::Number)
                .ok_or_else(|| format!("Rational to f64 failed: {}", r))
        }
        SteelVal::BigNum(bn) => {
            let s = bn.to_string();
            if let Ok(num) = s.parse::<serde_json::Number>() {
                Ok(Value::Number(num))
            } else {
                Ok(Value::String(s))
            }
        }
        SteelVal::StringV(s) => Ok(Value::String(s.to_string())),
        SteelVal::SymbolV(s) => Ok(Value::String(s.to_string())),
        SteelVal::CharV(c) => Ok(Value::String(c.to_string())),
        SteelVal::Void => Ok(Value::Null),
        SteelVal::ListV(l) => {
            let mut arr = Vec::with_capacity(l.len());
            for item in l {
                arr.push(steel_val_to_json(item)?);
            }
            Ok(Value::Array(arr))
        }
        SteelVal::VectorV(v) => {
            let mut arr = Vec::with_capacity(v.len());
            for item in v.iter() {
                arr.push(steel_val_to_json(item)?);
            }
            Ok(Value::Array(arr))
        }
        SteelVal::HashMapV(hm) => {
            let mut map = Map::new();
            for (k, v) in hm.iter() {
                let key = match k {
                    SteelVal::StringV(s) => s.to_string(),
                    SteelVal::SymbolV(s) => s.to_string(),
                    SteelVal::IntV(i) => i.to_string(),
                    other => format!("{}", other),
                };
                map.insert(key, steel_val_to_json(v)?);
            }
            Ok(Value::Object(map))
        }
        SteelVal::HashSetV(hs) => {
            let mut arr = Vec::with_capacity(hs.len());
            for item in hs.iter() {
                arr.push(steel_val_to_json(item)?);
            }
            Ok(Value::Array(arr))
        }
        other => Err(format!("Unsupported SteelVal type for JSON: {:?}", other)),
    }
}

/// Convert a [`serde_json::Value`] reference to a Scheme [`SteelVal`].
pub fn json_to_steel_val(val: &Value) -> SteelVal {
    match val {
        Value::Null => SteelVal::Void,
        Value::Bool(b) => SteelVal::BoolV(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SteelVal::IntV(i as isize)
            } else if let Some(u) = n.as_u64() {
                SteelVal::IntV(u as isize)
            } else if let Some(f) = n.as_f64() {
                SteelVal::NumV(f)
            } else {
                SteelVal::Void
            }
        }
        Value::String(s) => SteelVal::StringV(s.as_str().into()),
        Value::Array(arr) => {
            let items: Vec<SteelVal> = arr.iter().map(json_to_steel_val).collect();
            SteelVal::ListV(items.into())
        }
        Value::Object(obj) => {
            let mut map = HashMap::new();
            for (k, v) in obj {
                map.insert(SteelVal::StringV(k.as_str().into()), json_to_steel_val(v));
            }
            SteelVal::HashMapV(Gc::new(map).into())
        }
    }
}

/// Safely parses an `f32` from a `SteelVal` (accepts floats and integers).
pub fn parse_f32(val: &SteelVal) -> Result<f32, String> {
    match val {
        SteelVal::NumV(n) => Ok(*n as f32),
        SteelVal::IntV(i) => Ok(*i as f32),
        other => Err(format!("Expected number, got {:?}", other)),
    }
}

/// Safely parses an `isize` from a `SteelVal`.
pub fn parse_isize(val: &SteelVal) -> Result<isize, String> {
    match val {
        SteelVal::IntV(i) => Ok(*i),
        SteelVal::NumV(n) => Ok(*n as isize),
        other => Err(format!("Expected integer, got {:?}", other)),
    }
}

/// Parses a 3D coordinate vector `[x, y, z]` from a SteelVal list or vector of numbers.
pub fn parse_vec3(val: &SteelVal) -> Result<Vec3, String> {
    match val {
        SteelVal::ListV(list) if list.len() >= 3 => {
            let x = parse_f32(&list[0])?;
            let y = parse_f32(&list[1])?;
            let z = parse_f32(&list[2])?;
            Ok(vec3(x, y, z))
        }
        SteelVal::VectorV(vec) if vec.len() >= 3 => {
            let x = parse_f32(&vec[0])?;
            let y = parse_f32(&vec[1])?;
            let z = parse_f32(&vec[2])?;
            Ok(vec3(x, y, z))
        }
        other => Err(format!(
            "Expected list or vector of 3 numbers (x y z), got {:?}",
            other
        )),
    }
}

/// Parses a 2D vector `[w, d]` or `[x, y]` from a SteelVal list or vector of numbers.
pub fn parse_vec2(val: &SteelVal) -> Result<Vec2, String> {
    match val {
        SteelVal::ListV(list) if list.len() >= 2 => {
            let x = parse_f32(&list[0])?;
            let y = parse_f32(&list[1])?;
            Ok(vec2(x, y))
        }
        SteelVal::VectorV(vec) if vec.len() >= 2 => {
            let x = parse_f32(&vec[0])?;
            let y = parse_f32(&vec[1])?;
            Ok(vec2(x, y))
        }
        other => Err(format!(
            "Expected list or vector of 2 numbers, got {:?}",
            other
        )),
    }
}

/// Parses a Macroquad `Color` from a SteelVal list of `(r g b [a])`.
/// Automatically handles 0.0~1.0 normalized values or 0~255 integer ranges.
pub fn parse_color(val: &SteelVal) -> Result<Color, String> {
    let components: Vec<f32> = match val {
        SteelVal::ListV(list) => {
            let mut res = Vec::with_capacity(list.len());
            for item in list.iter() {
                res.push(parse_f32(item)?);
            }
            res
        }
        SteelVal::VectorV(vec) => {
            let mut res = Vec::with_capacity(vec.len());
            for item in vec.iter() {
                res.push(parse_f32(item)?);
            }
            res
        }
        other => {
            return Err(format!(
                "Expected list of color components (r g b [a]), got {:?}",
                other
            ))
        }
    };

    if components.len() < 3 {
        return Err(format!(
            "Color list requires at least 3 components (r g b), found {}",
            components.len()
        ));
    }

    let mut r = components[0];
    let mut g = components[1];
    let mut b = components[2];
    let mut a = if components.len() >= 4 {
        components[3]
    } else {
        1.0
    };

    // If any component is > 1.0, treat as 0-255 scale
    if r > 1.0 || g > 1.0 || b > 1.0 || a > 1.0 {
        r /= 255.0;
        g /= 255.0;
        b /= 255.0;
        if a > 1.0 {
            a /= 255.0;
        }
    }

    Ok(Color::new(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        a.clamp(0.0, 1.0),
    ))
}

/// Converts a `Vec3` to a Scheme list `'(x y z)`.
pub fn vec3_to_steel(v: Vec3) -> SteelVal {
    SteelVal::ListV(
        vec![
            SteelVal::NumV(v.x as f64),
            SteelVal::NumV(v.y as f64),
            SteelVal::NumV(v.z as f64),
        ]
        .into(),
    )
}

/// Converts a 3D float array `[f32; 3]` to a Scheme list `'(x y z)`.
pub fn slice3_to_steel(arr: [f32; 3]) -> SteelVal {
    SteelVal::ListV(
        vec![
            SteelVal::NumV(arr[0] as f64),
            SteelVal::NumV(arr[1] as f64),
            SteelVal::NumV(arr[2] as f64),
        ]
        .into(),
    )
}

/// Converts a 4D float array `[f32; 4]` to a Scheme list `'(r g b a)`.
pub fn slice4_to_steel(arr: [f32; 4]) -> SteelVal {
    SteelVal::ListV(
        vec![
            SteelVal::NumV(arr[0] as f64),
            SteelVal::NumV(arr[1] as f64),
            SteelVal::NumV(arr[2] as f64),
            SteelVal::NumV(arr[3] as f64),
        ]
        .into(),
    )
}

/// Converts an `EntityDesc` into an internal tuple `'(id shape pos size color label)`.
pub fn entity_to_raw_tuple(e: &EntityDesc) -> SteelVal {
    SteelVal::ListV(
        vec![
            SteelVal::StringV(e.id.clone().into()),
            SteelVal::StringV(e.shape.clone().into()),
            slice3_to_steel(e.pos),
            slice3_to_steel(e.size),
            slice4_to_steel(e.color),
            SteelVal::StringV(e.label.clone().into()),
        ]
        .into(),
    )
}

/// Converts JSON value to SteelVal.
pub fn json_to_val(j: &serde_json::Value) -> SteelVal {
    json_to_steel_val(j)
}

/// Converts SteelVal to JSON value.
pub fn val_to_json(v: &SteelVal) -> serde_json::Value {
    steel_val_to_json(v).unwrap_or(serde_json::Value::Null)
}

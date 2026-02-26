use std::collections::HashMap;

use super::types::{ParamDef, ParamValue};

pub struct ParamStore {
    pub defs: Vec<ParamDef>,
    pub values: HashMap<String, ParamValue>,
    pub changed: bool,
}

impl ParamStore {
    pub fn new() -> Self {
        Self {
            defs: Vec::new(),
            values: HashMap::new(),
            changed: false,
        }
    }

    pub fn load_from_defs(&mut self, defs: &[ParamDef]) {
        self.defs = defs.to_vec();
        self.values.clear();
        self.changed = false;
        for def in defs {
            self.values
                .insert(def.name().to_string(), def.default_value());
        }
    }

    pub fn set(&mut self, name: &str, value: ParamValue) {
        self.values.insert(name.to_string(), value);
        self.changed = true;
    }

    pub fn get(&self, name: &str) -> Option<&ParamValue> {
        self.values.get(name)
    }

    pub fn reset(&mut self, name: &str) {
        if let Some(def) = self.defs.iter().find(|d| d.name() == name) {
            self.values
                .insert(name.to_string(), def.default_value());
        }
    }

    pub fn reset_all(&mut self) {
        for def in &self.defs {
            self.values
                .insert(def.name().to_string(), def.default_value());
        }
    }

    /// Pack all param values into a fixed-size f32 array in definition order.
    pub fn pack_to_buffer(&self) -> [f32; 16] {
        let mut buf = [0.0f32; 16];
        let mut offset = 0;
        for def in &self.defs {
            if offset >= 16 {
                break;
            }
            if let Some(value) = self.values.get(def.name()) {
                let count = value.float_count();
                if offset + count <= 16 {
                    value.write_to(&mut buf, offset);
                    offset += count;
                }
            }
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::types::ParamDef;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn test_defs() -> Vec<ParamDef> {
        vec![
            ParamDef::Float {
                name: "speed".into(),
                default: 0.5,
                min: 0.0,
                max: 1.0,
            },
            ParamDef::Bool {
                name: "active".into(),
                default: true,
            },
            ParamDef::Color {
                name: "tint".into(),
                default: [1.0, 0.0, 0.0, 1.0],
            },
        ]
    }

    #[test]
    fn new_is_empty() {
        let s = ParamStore::new();
        assert!(s.defs.is_empty());
        assert!(s.values.is_empty());
        assert!(!s.changed);
    }

    #[test]
    fn load_from_defs_populates() {
        let mut s = ParamStore::new();
        s.load_from_defs(&test_defs());
        assert_eq!(s.defs.len(), 3);
        assert_eq!(s.values.len(), 3);
        assert!(!s.changed);
    }

    #[test]
    fn load_from_defs_default_values() {
        let mut s = ParamStore::new();
        s.load_from_defs(&test_defs());
        match s.get("speed") {
            Some(ParamValue::Float(v)) => assert!(approx_eq(*v, 0.5, 1e-6)),
            _ => panic!("expected Float"),
        }
        match s.get("active") {
            Some(ParamValue::Bool(v)) => assert!(*v),
            _ => panic!("expected Bool"),
        }
    }

    #[test]
    fn set_marks_changed() {
        let mut s = ParamStore::new();
        s.load_from_defs(&test_defs());
        assert!(!s.changed);
        s.set("speed", ParamValue::Float(0.8));
        assert!(s.changed);
    }

    #[test]
    fn get_existing_and_missing() {
        let mut s = ParamStore::new();
        s.load_from_defs(&test_defs());
        assert!(s.get("speed").is_some());
        assert!(s.get("nonexistent").is_none());
    }

    #[test]
    fn reset_single_param() {
        let mut s = ParamStore::new();
        s.load_from_defs(&test_defs());
        s.set("speed", ParamValue::Float(0.9));
        s.reset("speed");
        match s.get("speed") {
            Some(ParamValue::Float(v)) => assert!(approx_eq(*v, 0.5, 1e-6)),
            _ => panic!("expected Float"),
        }
    }

    #[test]
    fn reset_all_restores_defaults() {
        let mut s = ParamStore::new();
        s.load_from_defs(&test_defs());
        s.set("speed", ParamValue::Float(0.9));
        s.set("active", ParamValue::Bool(false));
        s.reset_all();
        match s.get("speed") {
            Some(ParamValue::Float(v)) => assert!(approx_eq(*v, 0.5, 1e-6)),
            _ => panic!("expected Float"),
        }
        match s.get("active") {
            Some(ParamValue::Bool(v)) => assert!(*v),
            _ => panic!("expected Bool"),
        }
    }

    #[test]
    fn pack_to_buffer_float_bool_color() {
        let mut s = ParamStore::new();
        s.load_from_defs(&test_defs());
        // defs order: speed (1 float), active (1 float), tint (4 floats) = 6 floats total
        let buf = s.pack_to_buffer();
        assert!(approx_eq(buf[0], 0.5, 1e-6)); // speed=0.5
        assert!(approx_eq(buf[1], 1.0, 1e-6)); // active=true -> 1.0
        assert!(approx_eq(buf[2], 1.0, 1e-6)); // tint.r=1.0
        assert!(approx_eq(buf[3], 0.0, 1e-6)); // tint.g=0.0
        assert!(approx_eq(buf[4], 0.0, 1e-6)); // tint.b=0.0
        assert!(approx_eq(buf[5], 1.0, 1e-6)); // tint.a=1.0
    }

    #[test]
    fn pack_to_buffer_truncates_at_16() {
        // Create 5 Color params (5*4=20 floats, exceeds 16)
        let defs: Vec<ParamDef> = (0..5)
            .map(|i| ParamDef::Color {
                name: format!("c{i}"),
                default: [1.0, 2.0, 3.0, 4.0],
            })
            .collect();
        let mut s = ParamStore::new();
        s.load_from_defs(&defs);
        let buf = s.pack_to_buffer();
        // 4 colors fit (4*4=16), 5th doesn't
        assert!(approx_eq(buf[12], 1.0, 1e-6)); // c3.r
        assert!(approx_eq(buf[15], 4.0, 1e-6)); // c3.a
    }

    #[test]
    fn pack_to_buffer_empty_store() {
        let s = ParamStore::new();
        let buf = s.pack_to_buffer();
        for v in buf {
            assert_eq!(v, 0.0);
        }
    }
}

use std::collections::HashMap;

use super::types::{ParamDef, ParamValue};

pub struct ParamStore {
    pub defs: Vec<ParamDef>,
    pub values: HashMap<String, ParamValue>,
}

impl ParamStore {
    pub fn new() -> Self {
        Self {
            defs: Vec::new(),
            values: HashMap::new(),
        }
    }

    pub fn load_from_defs(&mut self, defs: &[ParamDef]) {
        self.defs = defs.to_vec();
        self.values.clear();
        for def in defs {
            self.values
                .insert(def.name().to_string(), def.default_value());
        }
    }

    pub fn set(&mut self, name: &str, value: ParamValue) {
        self.values.insert(name.to_string(), value);
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

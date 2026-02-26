use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParamDef {
    Float {
        name: String,
        default: f32,
        min: f32,
        max: f32,
    },
    Color {
        name: String,
        default: [f32; 4],
    },
    Bool {
        name: String,
        default: bool,
    },
    Point2D {
        name: String,
        default: [f32; 2],
        min: [f32; 2],
        max: [f32; 2],
    },
}

impl ParamDef {
    pub fn name(&self) -> &str {
        match self {
            ParamDef::Float { name, .. } => name,
            ParamDef::Color { name, .. } => name,
            ParamDef::Bool { name, .. } => name,
            ParamDef::Point2D { name, .. } => name,
        }
    }

    pub fn default_value(&self) -> ParamValue {
        match self {
            ParamDef::Float { default, .. } => ParamValue::Float(*default),
            ParamDef::Color { default, .. } => ParamValue::Color(*default),
            ParamDef::Bool { default, .. } => ParamValue::Bool(*default),
            ParamDef::Point2D { default, .. } => ParamValue::Point2D(*default),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamValue {
    Float(f32),
    Color([f32; 4]),
    Bool(bool),
    Point2D([f32; 2]),
}

impl ParamValue {
    /// Number of f32 slots this value occupies in the uniform buffer.
    pub fn float_count(&self) -> usize {
        match self {
            ParamValue::Float(_) => 1,
            ParamValue::Color(_) => 4,
            ParamValue::Bool(_) => 1,
            ParamValue::Point2D(_) => 2,
        }
    }

    /// Write this value into a slice at the given offset.
    pub fn write_to(&self, buf: &mut [f32], offset: usize) {
        match self {
            ParamValue::Float(v) => buf[offset] = *v,
            ParamValue::Color(c) => buf[offset..offset + 4].copy_from_slice(c),
            ParamValue::Bool(b) => buf[offset] = if *b { 1.0 } else { 0.0 },
            ParamValue::Point2D(p) => buf[offset..offset + 2].copy_from_slice(p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn param_def_name_returns_correct_string() {
        let f = ParamDef::Float {
            name: "speed".into(),
            default: 0.5,
            min: 0.0,
            max: 1.0,
        };
        assert_eq!(f.name(), "speed");
        let c = ParamDef::Color {
            name: "tint".into(),
            default: [1.0, 0.0, 0.0, 1.0],
        };
        assert_eq!(c.name(), "tint");
        let b = ParamDef::Bool {
            name: "active".into(),
            default: true,
        };
        assert_eq!(b.name(), "active");
        let p = ParamDef::Point2D {
            name: "pos".into(),
            default: [0.0, 0.0],
            min: [-1.0, -1.0],
            max: [1.0, 1.0],
        };
        assert_eq!(p.name(), "pos");
    }

    #[test]
    fn param_def_default_value_float() {
        let f = ParamDef::Float {
            name: "x".into(),
            default: 0.75,
            min: 0.0,
            max: 1.0,
        };
        match f.default_value() {
            ParamValue::Float(v) => assert!(approx_eq(v, 0.75, 1e-6)),
            _ => panic!("expected Float"),
        }
    }

    #[test]
    fn param_def_default_value_color() {
        let c = ParamDef::Color {
            name: "c".into(),
            default: [0.1, 0.2, 0.3, 0.4],
        };
        match c.default_value() {
            ParamValue::Color(v) => {
                assert!(approx_eq(v[0], 0.1, 1e-6));
                assert!(approx_eq(v[3], 0.4, 1e-6));
            }
            _ => panic!("expected Color"),
        }
    }

    #[test]
    fn param_def_default_value_bool() {
        let b = ParamDef::Bool {
            name: "b".into(),
            default: true,
        };
        match b.default_value() {
            ParamValue::Bool(v) => assert!(v),
            _ => panic!("expected Bool"),
        }
    }

    #[test]
    fn param_def_default_value_point2d() {
        let p = ParamDef::Point2D {
            name: "p".into(),
            default: [0.5, -0.5],
            min: [-1.0, -1.0],
            max: [1.0, 1.0],
        };
        match p.default_value() {
            ParamValue::Point2D(v) => {
                assert!(approx_eq(v[0], 0.5, 1e-6));
                assert!(approx_eq(v[1], -0.5, 1e-6));
            }
            _ => panic!("expected Point2D"),
        }
    }

    #[test]
    fn param_value_float_count() {
        assert_eq!(ParamValue::Float(0.0).float_count(), 1);
        assert_eq!(ParamValue::Color([0.0; 4]).float_count(), 4);
        assert_eq!(ParamValue::Bool(false).float_count(), 1);
        assert_eq!(ParamValue::Point2D([0.0; 2]).float_count(), 2);
    }

    #[test]
    fn param_value_write_to_float() {
        let mut buf = [0.0f32; 4];
        ParamValue::Float(0.42).write_to(&mut buf, 1);
        assert!(approx_eq(buf[1], 0.42, 1e-6));
    }

    #[test]
    fn param_value_write_to_color_and_bool() {
        let mut buf = [0.0f32; 8];
        ParamValue::Color([0.1, 0.2, 0.3, 0.4]).write_to(&mut buf, 2);
        assert!(approx_eq(buf[2], 0.1, 1e-6));
        assert!(approx_eq(buf[5], 0.4, 1e-6));

        ParamValue::Bool(true).write_to(&mut buf, 0);
        assert!(approx_eq(buf[0], 1.0, 1e-6));
        ParamValue::Bool(false).write_to(&mut buf, 0);
        assert!(approx_eq(buf[0], 0.0, 1e-6));
    }
}

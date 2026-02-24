use serde::{Deserialize, Serialize};

use crate::params::ParamDef;

/// A .pfx effect definition (JSON format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PfxEffect {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub shader: String,
    #[serde(default)]
    pub inputs: Vec<ParamDef>,
}

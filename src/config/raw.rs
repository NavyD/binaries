use std::fmt;
use std::path::PathBuf;
use std::result;
use std::str;
use std::str::FromStr;

use anyhow::Result;

use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

/// The contents of the configuration file.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawConfig {
}

#[cfg(test)]
mod tests {
    use toml::Value;

    use super::*;

    #[test]
    fn test_name() -> Result<()> {
        let s = r#"
[bins.btm]
github="ClementTsang/bottom"
# version = "latest"
exe-glob = ""

[bins.tldr]


"#;
        let v = s.parse::<Value>()?;
        println!("{:?}", v);
        Ok(())
        
    }
}
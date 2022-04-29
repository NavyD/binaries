use std::str;

use serde::Deserialize;

/// The contents of the configuration file.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawConfig {}

#[cfg(test)]
mod tests {
    use anyhow::Result;
//     use toml::Value;

//     #[test]
//     fn test_name() -> Result<()> {
//         let s = r#"
// [bins.btm]
// github="ClementTsang/bottom"
// # version = "latest"
// exe-glob = ""

// [bins.tldr]


// "#;
//         let v = s.parse::<Value>()?;
//         println!("{:?}", v);
//         Ok(())
//     }
}

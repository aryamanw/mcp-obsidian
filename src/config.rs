use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub vault_path: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let vault_path = std::env::var("OBSIDIAN_VAULT")
            .map_err(|_| anyhow::anyhow!(
                "OBSIDIAN_VAULT environment variable not set. \
                 Set it to your Obsidian vault path in MCP config."
            ))?;

        let vault_path = PathBuf::from(&vault_path);
        if !vault_path.exists() {
            return Err(anyhow::anyhow!(
                "Vault not found at {}. Check OBSIDIAN_VAULT path.",
                vault_path.display()
            ));
        }
        if !vault_path.is_dir() {
            return Err(anyhow::anyhow!(
                "OBSIDIAN_VAULT ({}) is not a directory.",
                vault_path.display()
            ));
        }

        Ok(Self { vault_path })
    }
}

use clap::Parser;
use ledgerhold_cli::{Cli, default_db_path, run};
use ledgerhold_store::Store;

fn main() {
    let cli = Cli::parse();
    let result = (|| -> anyhow::Result<()> {
        let path = cli.db.clone().unwrap_or_else(default_db_path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut store = Store::open(&path)?;
        run(&cli, &mut store, &mut std::io::stdout().lock(), chrono::Utc::now().naive_utc())
    })();
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

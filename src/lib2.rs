use rayon::prelude::*;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Config<'a> {
    pub files: Vec<&'a str>,
    pub apparent_size: bool,
    pub total: bool,
    pub human_readable: bool,
    pub block_size: Option<u32>,
    pub si: bool,
    pub max_depth: Option<u32>,
}

#[derive(Debug)]
struct DirStats {
    depth: u32,
    path: PathBuf,
    bytes: u64,
    blocks: u64,
}

impl DirStats {
    pub fn run(mut self, config: &Config) -> io::Result<DirStats> {
        let mut children = Vec::new();
        for result in self.path.read_dir()? {
            let dent = result?;
            let metadata = dent.metadata()?;
            if metadata.is_file() {
                self.bytes += metadata.len();
                self.blocks += metadata.blocks();
            } else if metadata.is_dir() {
                let child = DirStats {
                    depth: self.depth + 1,
                    path: dent.path(),
                    bytes: 0,
                    blocks: 0,
                };
                children.push(child);
            }
        }
        let results = children
            .into_par_iter()
            .map(|d| d.run(config))
            .collect::<Vec<io::Result<DirStats>>>();
        let results: Vec<DirStats> = results.into_iter().collect::<Result<Vec<DirStats>, _>>()?;
        self.bytes += results.into_iter().map(|d| d.bytes).sum::<u64>();
        self.print(config);
        Ok(self)
    }

    fn print(&self, config: &Config) {
        if config.max_depth.map_or(true, |d| self.depth < d) {
            println!("{:>13?} {}", self.bytes, self.path.to_str().unwrap());
        }
    }
}

pub fn run(config: Config) {
    let tasks = config
        .files
        .iter()
        .map(|f| DirStats {
            depth: 0,
            path: PathBuf::from(f),
            bytes: 0,
            blocks: 0,
        })
        .collect::<Vec<_>>();

    tasks.into_par_iter().for_each(|task| {
        task.run(&config).expect("ok");
    });
}

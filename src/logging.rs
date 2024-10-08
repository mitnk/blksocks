use crate::LoggingConfig;
use flexi_logger::{DeferredNow, FileSpec, Logger, Record, WriteMode};
use std::io::Write;

fn custom_format(w: &mut dyn Write, now: &mut DeferredNow, record: &Record) -> std::io::Result<()> {
    write!(
        w,
        "[{}][{}] {}",
        now.format("%Y-%m-%d %H:%M:%S"),
        record.level(),
        &record.args()
    )
}

pub fn setup(logging_config: &LoggingConfig) {
    if !logging_config.enabled {
        return;
    }

    let file_size_limit = logging_config.file_size_limit_mb * 1_000_000;

    let logger = Logger::try_with_str("info")
        .unwrap()
        .format_for_files(custom_format)
        .log_to_file(
            FileSpec::default()
                .directory("/var/log/blksocks")
                .basename("blksocks"),
        )
        .write_mode(WriteMode::BufferAndFlush)
        .rotate(
            flexi_logger::Criterion::Size(file_size_limit),
            flexi_logger::Naming::Numbers,
            flexi_logger::Cleanup::KeepLogFiles(logging_config.rotate_count),
        )
        .start();

    match logger {
        Ok(_) => {}
        Err(e) => {
            eprintln!("init logging error: {}", e);
            std::process::exit(1);
        }
    }
}

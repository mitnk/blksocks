use flexi_logger::{DeferredNow, FileSpec, Logger, Record, WriteMode};
use std::io::Write;

fn custom_format(
    w: &mut dyn Write, now: &mut DeferredNow, record: &Record,
) -> std::io::Result<()> {
    write!(
        w,
        "[{}][{}] {}",
        now.format("%Y-%m-%d %H:%M:%S"),
        record.level(),
        &record.args()
    )
}

pub fn setup() {
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
            flexi_logger::Criterion::Size(5_000_000),  // MB
            flexi_logger::Naming::Numbers,
            flexi_logger::Cleanup::KeepLogFiles(3),
        )
        .start();

    match logger {
        Ok(_) => {},
        Err(e) => {
            eprintln!("init logging error: {}", e);
            std::process::exit(1);
        }
    }
}

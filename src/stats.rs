use std::collections::HashMap;
use time::{Duration, OffsetDateTime};
use std::net::IpAddr;

pub struct PacketStats {
    byte_count: u64,
    last_updated: OffsetDateTime,
}

pub fn update_stats(stats: &mut HashMap<IpAddr, PacketStats>, ip: IpAddr, bytes: u64) {
    let entry = stats.entry(ip).or_insert(PacketStats {
        byte_count: 0,
        last_updated: OffsetDateTime::now_utc(),
    });

    entry.byte_count += bytes;
    entry.last_updated = OffsetDateTime::now_utc();
}

pub fn expire_old_entries(stats: &mut HashMap<IpAddr, PacketStats>) {
    let expiry_threshold = OffsetDateTime::now_utc() - Duration::days(7);
    stats.retain(|_, entry| entry.last_updated > expiry_threshold);
}

pub fn get_top_ips(stats: &HashMap<IpAddr, PacketStats>) -> Vec<(IpAddr, u64)> {
    let mut stats_vec: Vec<_> = stats.iter().collect();
    stats_vec.sort_by(|a, b| b.1.byte_count.cmp(&a.1.byte_count));
    stats_vec.into_iter().take(80).map(|(&ip, stat)| (ip, stat.byte_count)).collect()
}

# blksocks: Black Sockets

A minimal [redsocks](https://github.com/darkk/redsocks) clone in Rust.

This tool allows you to redirect any TCP connection to a SOCKS v5 proxy using
your firewall.

## Build

Only support Linux, not work for Mac etc.

## Config

Setup log directory:
```
$ sudo mkdir -p /var/log/blksocks
```

Use `chown` to update its permission if `blksocks` run as non-root users.


The config file:
```
$ cat /etc/blksocks/config.toml
[network]
listen = "0.0.0.0:12345"
socks5 = "192.168.1.107:1080"

[logging]
enabled = true
file_size_limit_mb = 2
rotate_count = 5
```

> NOTE: May need to `listen` on `0.0.0.0` to make tproxy system work.

Use system env `TOKIO_WORKER_THREADS` to config the worker count, which
the default is the number of CPU cores.
```
$ export TOKIO_WORKER_THREADS=16
```

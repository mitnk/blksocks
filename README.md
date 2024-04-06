# blksocks: Black Sockets

A minimal [redsocks](https://github.com/darkk/redsocks) clone in Rust.

This tool allows you to redirect any TCP connection to a SOCKS v5 proxy using
your firewall.

## Build

Only support Linux, not work for Mac etc.

## Config

> NOTE: May need to `listen` on `0.0.0.0` to make tproxy system work.

```
$ cat /etc/blksocks/config.toml
listen = "127.0.0.1:12345"
socks5 = "192.168.1.107:1080"
```

Use system env `TOKIO_WORKER_THREADS` to config the worker count, which
the default is the number of CPU cores.
```
$ export TOKIO_WORKER_THREADS=16
```

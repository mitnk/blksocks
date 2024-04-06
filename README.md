# blksocks

Black Sockets: Learning [redsocks](https://github.com/darkk/redsocks) by clone it in Rust.

## Build

Only support Linux, not work for Mac etc.

## Config

> NOTE: I have to `listen` on `0.0.0.0` to make tproxy system work,
> Just like I using `redsocks`. Probably bad rules set in `iptables`?

```
$ cat /etc/blksocks/config.toml
listen = "127.0.0.1:12345"
socks5 = "192.168.1.107:1080"
```

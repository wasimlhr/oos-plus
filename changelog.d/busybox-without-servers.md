Removed:

- The image's busybox no longer holds `telnetd`, the telnet client, `httpd` or `tc`, and `nc` no longer listens
  (`nc -l`); nothing in OOS started them. busybox is now built from its source with a configuration of OOS
  ([network.md](docs/network.md), ADR 0078, #331).

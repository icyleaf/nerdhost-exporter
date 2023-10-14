# NerdHost exporter

A Prometheus Exporter for NerdHost SolusVM v1.

## Usage

Start the binary with `-h` to get the complete syntax. The parameters are:

| Parameter | Required | Valid values | Default | Description |
| -- | -- | -- | -- | -- |
| `-c`<br />`--config-path` | yes | *.json/yaml/toml | | Config path, see [examples](config/).
| `-m`<br />`--metrics-server` | no | HOST:PORT | 0.0.0.0:9103 | Specify the serivce with port. This is the target your Prometheus instance should point to.
| `--metrics-path` | no | URI Path | /metrics | This is the path of URI, must starts with `/` char.

Once started, the tool will listen on the specified port (or the default one, 9103, if not specified) and return a Prometheus valid response at the url `/metrics`. So to check if the tool is working properly simply browse the `http://0.0.0.0:9103` (or whichever address with port you choose).

## Deopy

### Docker

```bash
docker run --name nerdhost-exporter \
  -p 9103:9103 \
  -v /etc/nerdhost/config.yml:/etc/nerdhost/config.yml \
  ghcr.io/icyleaf/bandwagon-exporter:snapshot \
  --config-path=/etc/nerdhost/config.yml
```

### Docker Compose

```yaml
version: "3"
services:
  nerdhost-exporter:
    restart: unless-stopped
    image: ghcr.io/icyleaf/nerdhost-exporter:snapshot
    command:
      - '--config-path=/etc/nerdhost/config.yml'
    volumes:
      - ./config/config.yml:/etc/nerdhost/config.yml
    ports:
      - "9103:9103"
```

## Prometheus Metrics

```text
# HELP nerdhost_bandwith Bandwith date information
# TYPE nerdhost_bandwith gauge
nerdhost_bandwith{free="4293772260560",hostname="vm12345",total="4294967296000",used="1195035440",used_percent="0"} 1
# HELP nerdhost_disk Disk information
# TYPE nerdhost_disk gauge
nerdhost_disk{free="26843545600",hostname="vm12345",total="26843545600",used="0",used_percent="0"} 1
# HELP nerdhost_server_status Server status information
# TYPE nerdhost_server_status gauge
nerdhost_server_status{hostname="vm12345",ipaddress="1.2.3.4",vm_status="online"} 1
```

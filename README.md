# NerdHost exporter

A Prometheus Exporter for NerdHost SolusVM v1.

## Usage

Start the binary with `-h` to get the complete syntax. The parameters are:

| Parameter | Required | Valid values | Default | Description |
| -- | -- | -- | -- | -- |
| `-c`<br />`--config-path` | yes | *.json/yaml/toml | | Config path, see [examples](config/). |
| `-m`<br />`--metrics-server` | no | HOST:PORT | 0.0.0.0:9103 | Specify the serivce with port. This is the target your Prometheus instance should point to. |
| `--metrics-path` | no | URI Path | /metrics | This is the path of URI, must starts with `/` char. |

Once started, the tool will listen on the specified port (or the default one, 9103, if not specified) and return a Prometheus valid response at the url `/metrics`. So to check if the tool is working properly simply browse the `http://0.0.0.0:9103` (or whichever address with port you choose).

## Configuration

A node is configured with its API credentials. You may optionally give each node a
`name` to identify it in logs and metrics; if omitted, the hostname reported by the
node API is used when available, otherwise a generated label like `node-0` is used.
Two nodes cannot share a `name`: their outcomes would be reported as one node, which
makes the exporter exit at startup, as does any configuration it cannot serve.
The configuration is read and checked once at startup, and a configuration that
cannot serve a node makes the exporter exit instead of serving.

```yaml
endpoint: https://ctrl.racknerd.com

# Optional, these are the defaults.
node_timeout_seconds: 5     # how long one call to a node may take
scrape_deadline_seconds: 8  # how long a whole /metrics request may take
max_concurrency: 8          # how many node calls may be in flight at once

nodes:
  - api_key: 1234567890abcdef1
    api_secret: 1111111111111111111111111111111111111111111111111111111111111111
    name: my-node
```

`node_timeout_seconds` and `scrape_deadline_seconds` both need to stay under your
Prometheus `scrape_timeout` (10s by default), or Prometheus gives up before the
exporter answers. `node_timeout_seconds` also has to stay under
`scrape_deadline_seconds`, so that a single call can reach its own timeout before the
whole Scrape is cut off.

## Deploy

### Docker

```bash
docker run --name nerdhost-exporter \
  -p 9103:9103 \
  -v /etc/nerdhost/config.yml:/etc/nerdhost/config.yml \
  ghcr.io/icyleaf/nerdhost-exporter:snapshot \
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
# HELP nerdhost_bandwidth_free Bandwidth free in bytes
# TYPE nerdhost_bandwidth_free gauge
nerdhost_bandwidth_free{hostname="vm12345"} 2665184493568
nerdhost_bandwidth_free{hostname="vm67890"} 4100256169984
# HELP nerdhost_bandwidth_total Bandwidth total in bytes
# TYPE nerdhost_bandwidth_total gauge
nerdhost_bandwidth_total{hostname="vm12345"} 2684354560000
nerdhost_bandwidth_total{hostname="vm67890"} 4294967296000
# HELP nerdhost_bandwidth_used Bandwidth used in bytes
# TYPE nerdhost_bandwidth_used gauge
nerdhost_bandwidth_used{hostname="vm12345"} 19170066432
nerdhost_bandwidth_used{hostname="vm67890"} 194711126016
# HELP nerdhost_bandwidth_used_percent Bandwidth used, in percent of the total
# TYPE nerdhost_bandwidth_used_percent gauge
nerdhost_bandwidth_used_percent{hostname="vm12345"} 1
nerdhost_bandwidth_used_percent{hostname="vm67890"} 5
# HELP nerdhost_disk_free Disk free in bytes
# TYPE nerdhost_disk_free gauge
nerdhost_disk_free{hostname="vm12345"} 16584278016
nerdhost_disk_free{hostname="vm67890"} 16626221056
# HELP nerdhost_disk_total Disk total in bytes
# TYPE nerdhost_disk_total gauge
nerdhost_disk_total{hostname="vm12345"} 37580963840
nerdhost_disk_total{hostname="vm67890"} 26843545600
# HELP nerdhost_disk_used Disk used in bytes
# TYPE nerdhost_disk_used gauge
nerdhost_disk_used{hostname="vm12345"} 20996685824
nerdhost_disk_used{hostname="vm67890"} 10217324544
# HELP nerdhost_disk_used_percent Disk used, in percent of the total
# TYPE nerdhost_disk_used_percent gauge
nerdhost_disk_used_percent{hostname="vm12345"} 56
nerdhost_disk_used_percent{hostname="vm67890"} 38
# HELP nerdhost_memory_free Memory free in bytes
# TYPE nerdhost_memory_free gauge
nerdhost_memory_free{hostname="vm12345"} 466616320
nerdhost_memory_free{hostname="vm67890"} 507510784
# HELP nerdhost_memory_total Memory total in bytes
# TYPE nerdhost_memory_total gauge
nerdhost_memory_total{hostname="vm12345"} 2147483648
nerdhost_memory_total{hostname="vm67890"} 1073741824
# HELP nerdhost_memory_used Memory used in bytes
# TYPE nerdhost_memory_used gauge
nerdhost_memory_used{hostname="vm12345"} 1680867328
nerdhost_memory_used{hostname="vm67890"} 566231040
# HELP nerdhost_memory_used_percent Memory used, in percent of the total
# TYPE nerdhost_memory_used_percent gauge
nerdhost_memory_used_percent{hostname="vm12345"} 78
nerdhost_memory_used_percent{hostname="vm67890"} 53
# HELP nerdhost_node_info Node identity and image as the node API reports it. Always 1.
# TYPE nerdhost_node_info gauge
nerdhost_node_info{hostname="vm12345",ipaddress="1.2.3.4",os="Debian 10 64 Bit",template="linux-debian-10-x86_64-gen2-v1"} 1
nerdhost_node_info{hostname="vm67890",ipaddress="1.2.3.5",os="Debian 12 64 bit",template="linux-debian-12-x86_64-gen2-v1"} 1
# HELP nerdhost_node_scrape_results Node calls that ended this way in this Scrape. reason is empty when the call answered.
# TYPE nerdhost_node_scrape_results gauge
nerdhost_node_scrape_results{call="status",node="node1",reason=""} 1
nerdhost_node_scrape_results{call="status",node="node2",reason=""} 1
nerdhost_node_scrape_results{call="usage",node="node1",reason=""} 1
nerdhost_node_scrape_results{call="usage",node="node2",reason=""} 1
# HELP nerdhost_node_up 1 when every call for the node answered in this Scrape, 0 otherwise
# TYPE nerdhost_node_up gauge
nerdhost_node_up{node="node1"} 1
nerdhost_node_up{node="node2"} 1
# HELP nerdhost_server_status Server status: 1 when online, 0 otherwise
# TYPE nerdhost_server_status gauge
nerdhost_server_status{hostname="vm12345",vm_status="online"} 1
nerdhost_server_status{hostname="vm67890",vm_status="online"} 1
```

The usage call brings back bandwidth, disk and memory together, so those three
families always appear as a set for a node whose call answered. The node API reports
each as `total,used,free,percent`; the exporter publishes the figures in bytes plus
the API's own percentage, so nothing has to guess how it rounds.

`nerdhost_node_up` is the one series every configured node always has, keyed by the
same identity as the `node` label above. A node whose key is refused still has a
series at 0, which is what makes `nerdhost_node_up == 0` and `absent()` mean different
things: the node answered nothing, versus the exporter is not running at all.

`nerdhost_node_info` carries the image the node API reports for it (`template`, `os`)
alongside its `hostname`, so panels and alerts can group by OS without a second
endpoint.

### What the series mean

A node's usage and its status are fetched by two separate calls, and they answer
independently, which is why outcomes are reported per `call`. A call that did not
answer is counted under its `reason`:

- `transport` — nothing came back: DNS, TCP, TLS, the node timeout, or the Scrape deadline.
- `api` — the node API itself refused, and said why. The message goes to the log.
- `parse` — the node API answered with something that could not be read.

`nerdhost_node_scrape_results` is a count per outcome, not a 1/0 flag: with several
nodes, a flag would let one node's outcome overwrite another's. Summing every series
gives twice the number of configured nodes.

`nerdhost_server_status` and the usage series are only reported for a node whose
hostname came back, which is why the status reply never carries one of its own: a
reading that cannot be attributed to a node is left out rather than labelled
ambiguously. `nerdhost_node_info` is the only series carrying `ipaddress`.

Every response is built from that one request's calls, so a usage or status series
disappears as soon as the node stops reporting it, rather than lingering with a stale
value. `nerdhost_node_up` is the exception, and it says so by dropping to 0.

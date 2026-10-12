# racknerd-exporter

A Prometheus Exporter for RackNerd's VPS nodes. RackNerd is the only
provider this exporter is meant for.

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
`name` to identify it in logs and metrics; if omitted (an empty `name` counts as
omitted), the hostname reported by the node API is used when available, otherwise a
generated label like `node-0` is used.
Two nodes cannot share a `name`: their outcomes would be reported as one node, which
makes the exporter exit at startup, as does any configuration it cannot serve.
The configuration is read and checked once at startup, and a configuration that
cannot serve a node makes the exporter exit instead of serving.

`endpoint` defaults to `https://ctrl.racknerd.com`, the host RackNerd serves the node
API from (SolusVM v1). It is a host, not a full URL: the exporter appends the call
path itself, so `/api/client/command.php` does not belong here.

```yaml
endpoint: https://ctrl.racknerd.com   # optional, this is the default

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
docker run --name racknerd-exporter \
  -p 9103:9103 \
  -v /etc/racknerd/config.yml:/etc/racknerd/config.yml \
  ghcr.io/icyleaf/racknerd-exporter:snapshot \
  --config-path=/etc/racknerd/config.yml
```

### Docker Compose

```yaml
version: "3"
services:
  racknerd-exporter:
    restart: unless-stopped
    image: ghcr.io/icyleaf/racknerd-exporter:snapshot
    command:
      - '--config-path=/etc/racknerd/config.yml'
    volumes:
      - ./config/config.yml:/etc/racknerd/config.yml
    ports:
      - "9103:9103"
```

## Prometheus Metrics

```text
# HELP racknerd_bandwidth_free Bandwidth free in bytes
# TYPE racknerd_bandwidth_free gauge
racknerd_bandwidth_free{hostname="vm12345",node="node1"} 2665171910656
racknerd_bandwidth_free{hostname="vm67890",node="node2"} 4100148166656
# HELP racknerd_bandwidth_total Bandwidth total in bytes
# TYPE racknerd_bandwidth_total gauge
racknerd_bandwidth_total{hostname="vm12345",node="node1"} 2684354560000
racknerd_bandwidth_total{hostname="vm67890",node="node2"} 4294967296000
# HELP racknerd_bandwidth_used Bandwidth used in bytes
# TYPE racknerd_bandwidth_used gauge
racknerd_bandwidth_used{hostname="vm12345",node="node1"} 19182649344
racknerd_bandwidth_used{hostname="vm67890",node="node2"} 194819129344
# HELP racknerd_bandwidth_used_percent Bandwidth used, in percent of the total
# TYPE racknerd_bandwidth_used_percent gauge
racknerd_bandwidth_used_percent{hostname="vm12345",node="node1"} 1
racknerd_bandwidth_used_percent{hostname="vm67890",node="node2"} 5
# HELP racknerd_disk_free Disk free in bytes
# TYPE racknerd_disk_free gauge
racknerd_disk_free{hostname="vm12345",node="node1"} 16584278016
racknerd_disk_free{hostname="vm67890",node="node2"} 16626221056
# HELP racknerd_disk_total Disk total in bytes
# TYPE racknerd_disk_total gauge
racknerd_disk_total{hostname="vm12345",node="node1"} 37580963840
racknerd_disk_total{hostname="vm67890",node="node2"} 26843545600
# HELP racknerd_disk_used Disk used in bytes
# TYPE racknerd_disk_used gauge
racknerd_disk_used{hostname="vm12345",node="node1"} 20996685824
racknerd_disk_used{hostname="vm67890",node="node2"} 10217324544
# HELP racknerd_disk_used_percent Disk used, in percent of the total
# TYPE racknerd_disk_used_percent gauge
racknerd_disk_used_percent{hostname="vm12345",node="node1"} 56
racknerd_disk_used_percent{hostname="vm67890",node="node2"} 38
# HELP racknerd_memory_free Memory free in bytes
# TYPE racknerd_memory_free gauge
racknerd_memory_free{hostname="vm12345",node="node1"} 466616320
racknerd_memory_free{hostname="vm67890",node="node2"} 508559360
# HELP racknerd_memory_total Memory total in bytes
# TYPE racknerd_memory_total gauge
racknerd_memory_total{hostname="vm12345",node="node1"} 2147483648
racknerd_memory_total{hostname="vm67890",node="node2"} 1073741824
# HELP racknerd_memory_used Memory used in bytes
# TYPE racknerd_memory_used gauge
racknerd_memory_used{hostname="vm12345",node="node1"} 1680867328
racknerd_memory_used{hostname="vm67890",node="node2"} 565182464
# HELP racknerd_memory_used_percent Memory used, in percent of the total
# TYPE racknerd_memory_used_percent gauge
racknerd_memory_used_percent{hostname="vm12345",node="node1"} 78
racknerd_memory_used_percent{hostname="vm67890",node="node2"} 53
# HELP racknerd_node_info Node identity and image as the node API reports it. Always 1.
# TYPE racknerd_node_info gauge
racknerd_node_info{hostname="vm12345",ipaddress="1.2.3.4",node="node1",os="Debian 10 64 Bit",template="linux-debian-10-x86_64-gen2-v1"} 1
racknerd_node_info{hostname="vm67890",ipaddress="1.2.3.5",node="node2",os="Debian 12 64 bit",template="linux-debian-12-x86_64-gen2-v1"} 1
# HELP racknerd_node_scrape_results Node calls that ended this way in this Scrape. reason is empty when the call answered.
# TYPE racknerd_node_scrape_results gauge
racknerd_node_scrape_results{call="status",node="node1",reason=""} 1
racknerd_node_scrape_results{call="status",node="node2",reason=""} 1
racknerd_node_scrape_results{call="usage",node="node1",reason=""} 1
racknerd_node_scrape_results{call="usage",node="node2",reason=""} 1
# HELP racknerd_node_up 1 when every call for the node answered in this Scrape, 0 otherwise
# TYPE racknerd_node_up gauge
racknerd_node_up{node="node1"} 1
racknerd_node_up{node="node2"} 1
# HELP racknerd_server_status Server status: 1 when online, 0 otherwise
# TYPE racknerd_server_status gauge
racknerd_server_status{hostname="vm12345",node="node1",vm_status="online"} 1
racknerd_server_status{hostname="vm67890",node="node2",vm_status="online"} 1
```

Every series that names a node carries the same two labels. `node` is what the Scrape
calls the node — the `name` you configured, or the hostname the node API gave when
you configured none — and it is what makes the series joinable: the usage series of a
node and its `racknerd_node_up` carry one identity, so a panel can group by `node`
without assuming a name and a hostname are spelled the same way. `hostname` is the
node API's own string for the node, kept beside it. Only `racknerd_node_up` and
`racknerd_node_scrape_results` go without a `hostname`: they are reported for nodes
whose calls may never have answered, and a call that did not answer never learned one.
A label is only as steady as what it is built from, so a configured `name` is the
stable choice; an API hostname belongs to the API and can change under you.

Two configured nodes that report the same `hostname` are reported as two series, told
apart by `node`: each is a separate credential, and one reading never overwrites the
other. A node the API gave an empty hostname for is still reported, with
`hostname=""`, rather than dropped — its call answered, so the figures are real.

The usage call brings back bandwidth, disk and memory together, so those three
families always appear as a set for a node whose call answered. The node API reports
each as `total,used,free,percent`; the exporter publishes the figures in bytes plus
the API's own percentage, so nothing has to guess how it rounds.

`racknerd_node_up` is the one series every configured node always has, keyed by the
same identity as the `node` label above. A node whose key is refused still has a
series at 0, which is what makes `racknerd_node_up == 0` and `absent()` mean different
things: the node answered nothing, versus the exporter is not running at all.

`racknerd_node_info` carries the image the node API reports for it (`template`, `os`)
alongside its `hostname`, so panels and alerts can group by OS without a second
endpoint.

### What the series mean

A node's usage and its status are fetched by two separate calls, and they answer
independently, which is why outcomes are reported per `call`. A call that did not
answer is counted under its `reason`:

- `transport` — nothing came back: DNS, TCP, TLS, the node timeout, or the Scrape deadline.
- `api` — the node API itself refused, and said why. The message goes to the log.
- `parse` — the node API answered with something that could not be read.

`racknerd_node_scrape_results` is a count per outcome, not a 1/0 flag: with several
nodes, a flag would let one node's outcome overwrite another's. Summing every series
gives twice the number of configured nodes.

`racknerd_server_status` and the usage series are only reported for a node whose
usage call answered, which is why the status reply never carries a hostname of its
own: a
reading that cannot be attributed to a node is left out rather than labelled
ambiguously. `racknerd_node_info` is the only series carrying `ipaddress`.

Every response is built from that one request's calls, so a usage or status series
disappears as soon as the node stops reporting it, rather than lingering with a stale
value. `racknerd_node_up` is the exception, and it says so by dropping to 0.

# racknerd-exporter

A Prometheus exporter that reports the resource usage of SolusVM v1 nodes to a metrics scraper.

## Language

**Node**:
One SolusVM VPS the exporter watches. Its API credentials are listed in the configuration.
_Avoid_: server, VM, instance, host

**Node name**:
The optional label the configuration gives a Node. It is the identity a Node is reported under, in logs and in the `node` label, whether or not its calls answered.
_Avoid_: alias, id, hostname

**Hostname**:
The name the node API reports for a Node. Only a call that answered carries one, so it labels the usage and status series, and stands in as the Node's identity when the configuration gives no Node name.
_Avoid_: node name, alias, id

**Scrape**:
One pass over every Node, triggered by a request for the metrics path. The response describes only what that Scrape obtained.
_Avoid_: poll, collection, gather, refresh

**Node sample**:
What one Node contributes to a Scrape: its usage figures and its status.
_Avoid_: ServerInfo, ServerStatus, reading, result

**Node failure**:
A call a Scrape did not get a result from. Kept per call, because a Node's usage and its status can fail independently.
_Avoid_: error, exception, fault

**Node identity**:
What a Node's outcomes are reported under: its Node name, else the Hostname its calls returned, else a position-based label such as `node-0`. It is why a failed call can still be attributed to the Node that failed.
_Avoid_: node label, identifier, key

**Scrape deadline**:
The longest a Scrape may take. Nodes still unanswered when it passes count as Node failures.
_Avoid_: timeout (that is one call's limit), scrape_timeout (that is the metrics scraper's own limit)

**Usage**:
The total, used and free triple the node API reports for bandwidth, disk or memory.
_Avoid_: quota, quota usage, stats, bandwith

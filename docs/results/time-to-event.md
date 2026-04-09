For `time-to-event-proxy`, large coarsening distortion is expected. In coarsened mode, clinical timestamps are reduced to year-level anchors, while the metric depends on day-level differences (`days_to_event`) and a hard `max_days` filter (365 in this run). That combination changes both cohort membership and the mean substantially: more patients pass the window under coarsening (`n=168` vs `n=102`), and the estimated mean time to event shifts upward (`134.70` vs `52.70` days).

Therefore with Coarsening enabled (which gives better privacy protection), the value of this kind of query is not there anymore.

With coarsening disabled the value stays exact and only has some negligible DP noise.
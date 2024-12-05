CREATE MATERIALIZED VIEW csi_metrics_injector_droppedframes_count_1sec
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('1 second', timestamp) AS bucket,
    sensor_id,
    COUNT(*)
FROM csi_telemetry
WHERE device_type = 0
AND message_type = 2
GROUP BY bucket, sensor_id;

SELECT add_continuous_aggregate_policy(
    'csi_metrics_injector_droppedframes_count_1sec',
    start_offset => INTERVAL '4 minutes',
    end_offset => INTERVAL '1 minute',
    schedule_interval => INTERVAL '5 minutes'
);

create index csi_metrics_injector_droppedframes_count_1sec_injector_idx on csi_metrics_injector_droppedframes_count_1sec (sensor_id, bucket);
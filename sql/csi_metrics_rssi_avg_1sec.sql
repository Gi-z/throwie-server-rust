CREATE MATERIALIZED VIEW csi_metrics_rssi_avg_1sec
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('1 second', timestamp) AS bucket,
    sensor_id,
    antenna,
    ROUND(AVG(rssi), 1) AS rssi
FROM csi_data
GROUP BY bucket, sensor_id, antenna;

create index csi_metrics_rssi_avg_1sec_sensor_idx on csi_metrics_rssi_avg_1sec (sensor_id, antenna, bucket);
create index csi_metrics_rssi_avg_1sec_sensor_antenna_idx on csi_metrics_rssi_avg_1sec (sensor_id, antenna, bucket);
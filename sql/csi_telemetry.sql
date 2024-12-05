CREATE TABLE csi_telemetry (
    sensor_id CHAR(6),
    timestamp TIMESTAMPTZ NOT NULL,
    sequence_identifier INTEGER,
    uptime_ms INTEGER,
    version CHAR(8),
    device_type SMALLINT,
    message_type SMALLINT,
    is_eth BOOL,
    PRIMARY KEY (sensor_id, timestamp)
);
SELECT create_hypertable('csi_telemetry', 'timestamp');
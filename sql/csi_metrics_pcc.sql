CREATE TABLE csi_metrics_pcc (
    sensor_id VARCHAR,
    timestamp TIMESTAMP NOT NULL,
    pcc REAL NOT NULL,
    PRIMARY KEY (sensor_id, timestamp),
);
SELECT create_hypertable('csi_data', 'timestamp');
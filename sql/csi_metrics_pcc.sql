CREATE TABLE csi_metrics_pcc (
     sensor_id MACADDR,
     timestamp TIMESTAMP NOT NULL,
     level SMALLINT NOT NULL,
     pcc REAL NOT NULL,
     PRIMARY KEY (sensor_id, timestamp, level)
);

SELECT create_hypertable('csi_metrics_pcc', 'timestamp');
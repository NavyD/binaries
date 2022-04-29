DROP TABLE IF EXISTS updated_info;

CREATE TABLE IF NOT EXISTS `updated_info` (
    `id` INTEGER PRIMARY KEY AUTOINCREMENT,
    `name` VARCHAR(20) NOT NULL,
    `version` VARCHAR(20) NOT NULL,
    -- json for source enum struct
    `source` varchar(1024) NOT NULL,
    `url` varchar(1024) NOT NULL,
    `create_time` datetime NOT NULL,
    `updated_time` datetime NOT NULL,
    UNIQUE(`name`, `version`)
);
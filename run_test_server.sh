sed -i 's/limit_max_cross.*/limit_max_cross = 201574/' test_config.toml
sed -i 's/limit_max_coax.*/limit_max_coax = 201574/' test_config.toml
sed -i 's/control_mode.*/control_mode = "Tracking"/' test_config.toml
sed -i 's/limit_min_coax.*/limit_min_coax = 0/' test_config.toml
sed -i 's/limit_min_cross.*/limit_min_cross = 0/' test_config.toml

cargo run -- test_config.toml

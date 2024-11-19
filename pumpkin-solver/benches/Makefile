sync_examples_set:
	rsync -r --stats --progress ./examples-set tu-delftblue:/scratch/rbaauw/examples-set

sync_binary_and_config:
	cp ../../target/x86_64-unknown-linux-musl/release/intsat_benchmarks ./
	rsync --stats --progress ./intsat_benchmarks ./gourd.toml tu-delftblue:~/pumpkin

sync_results_data:
	rsync -r --stats --progress tu-delftblue:~/pumpkin/experiments ./results

show_errored_files:
	cd results/3/0 && find -type f -name stderr | xargs ls -lah -S
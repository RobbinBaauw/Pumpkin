sync_examples_set:
	rsync -r --stats --progress ./examples-set tu-delftblue:/scratch/rbaauw/examples-set

sync_binary_and_config:
	cp ../../target/x86_64-unknown-linux-musl/release/intsat_benchmarks ./
	rsync --stats --progress ./intsat_benchmarks ./intsat_og ./gourd.toml tu-delftblue:~/pumpkin

sync_results_data:
	rsync -r --stats --progress --exclude 'gourd_*.out' --exclude 'gourd_*.err' tu-delftblue:~/pumpkin/experiments ./results

show_errored_files:
	find -type f -name stderr -size +0 | xargs tail -n +1
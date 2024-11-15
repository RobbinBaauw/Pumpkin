#!/usr/bin/env python3
import csv
import sys

if __name__ == "__main__":
    _, job_output, script_output = sys.argv

    stats = {}
    with open(job_output) as job_output_f:
        for line in job_output_f.read().splitlines():
            if line.startswith("stat_"):
                _, stat = line.split(" ")
                name, value = stat.split("=")
                stats[name] = value

    with open(script_output, "w") as script_output_f:
        csv.writer(script_output_f).writerows(list(stats.items()))

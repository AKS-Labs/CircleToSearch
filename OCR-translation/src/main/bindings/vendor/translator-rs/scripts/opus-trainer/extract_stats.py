#!/usr/bin/env python3
"""
Computing stats for biases

This file extracts the average values for biases in order to properly quantize a model.
It is a direct copy from the marian-fork, that is now placed here.

https://github.com/browsermt/marian-dev/blob/2781d735d4a10dca876d61be587afdab2726293c/scripts/alphas/extract_stats.py
"""

from sys import argv, exit
import numpy as np

if len(argv) != 4:
    print("Usage:", argv[0], "dumpfile input_model output_model")
    exit(1)

input_file = open(argv[1], "r")

matrices_maxabs = dict()
matrices_means = dict()
matrices_stddev = dict()
matrices_meansAbs = dict()
matrices_stddevAbs = dict()
for line in input_file:
    if "tcmalloc" in line:
        continue
    _, name, _, meanAbs, _, stdAbs, _, mean, _, stddev, _, maxAbs = line.strip().split()
    name = name.split("::")[-1]
    if name not in matrices_maxabs:
        matrices_maxabs[name] = []
        matrices_means[name] = []
        matrices_stddev[name] = []
        matrices_meansAbs[name] = []
        matrices_stddevAbs[name] = []

    matrices_maxabs[name].append(float(maxAbs))
    matrices_means[name].append(float(mean))
    matrices_stddev[name].append(float(stddev))
    matrices_meansAbs[name].append(float(meanAbs))
    matrices_stddevAbs[name].append(float(stdAbs))

input_file.close()

for name in matrices_maxabs:
    if "QuantMultA" in name:
        print(
            name,
            "MaxAbsMean:",
            np.mean(matrices_maxabs[name]),
            "MaxAbsStdDev:",
            np.std(matrices_maxabs[name]),
        )
        print(
            name,
            "MeanMean   ",
            np.mean(matrices_means[name]),
            "MeanStd      ",
            np.std(matrices_means[name]),
        )
        print(
            name,
            "Stdmean    ",
            np.std(matrices_stddev[name]),
            "StdStd       ",
            np.std(matrices_stddev[name]),
        )
        print(
            name,
            "MeanAbsMean",
            np.mean(matrices_meansAbs[name]),
            "MeanAbsStd   ",
            np.std(matrices_meansAbs[name]),
        )
        print(
            name,
            "StdAbsmean ",
            np.std(matrices_stddevAbs[name]),
            "StdAbsStd    ",
            np.std(matrices_stddevAbs[name]),
        )
        # print(name, "MaxAbsMean:", np.mean(matrices_maxabs[name]), "MaxAbsStdDev:", np.std(matrices_maxabs[name]))


model_file = np.load(argv[2])
model_file_dict = dict(model_file)
for name in matrices_maxabs:
    if "QuantMultA" in name:
        res = np.array((1,), dtype=np.float32)
        res[0] = 127 / (np.mean(matrices_maxabs[name]) + 1.1 * np.std(matrices_maxabs[name]))
        model_file_dict[name] = res
# Make sure that the decoder works whether a shortlist has been generated or not:
# This is necessary because when there is a shortlist, the matrix names are different.
# The Wemb matrix becomes "none"
if "Wemb_QuantMultA" in model_file_dict:
    model_file_dict["none_QuantMultA"] = model_file_dict["Wemb_QuantMultA"]
elif "none_QuantMultA" in model_file_dict:
    model_file_dict["Wemb_QuantMultA"] = model_file_dict["none_QuantMultA"]

np.savez(argv[3], **model_file_dict)

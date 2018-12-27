# given an input that is the IP output of sample_once, and a base address for the program, calculates the relative IPs.

from __future__ import print_function

import sys

base = int(sys.argv[1], 16)

ips = []
for line in sys.stdin.readlines():
    for token in line.split():
        try:
            ips.append(int(token.strip(), 16))
        except ValueError:
            pass

for ip in ips:
    print(hex(ip-base))

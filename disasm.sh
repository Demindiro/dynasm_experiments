#!/bin/sh
exec objdump -D -b binary -Mintel,x86-64 -m i386 $1

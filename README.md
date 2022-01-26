# JIT compiler & interpreter for ´

This program is a quick experiment to (naïvely) test the performance difference
between interpreters and JIT compilers for simple programs.

## ´

´ is a derivative of [\`]. It is modified such that finite loops are possible.

Instructions still use backticks since forward ticks are not ASCII and hence annoying.

## Instructions

A and B are integer constants, \[X\] denotes an address dereference,  V is the last assigned
value, P is the instruction pointer.

| Syntax |       Function      |
| ------ | ------------------- |
| A`+B   | [A] += B, V = [A]   |
| A`B    | [A] += [B], V = [A] |
| +A`+B  | [A] == V ? P += B   |
| +A`B   | [A] == V ? P += [B] |
| other  | ignored             |

[\`]: https://esolangs.org/wiki/%60

# C examples for libhi

This directory contains various examples for using the C interfacae of libhi.

## Notes

To get proper completion while developing these examples, I used https://github.com/nickdiego/compiledb to generate a `compile_commands.json` from the makefile (and thus allowing my LSP client to find all references).
It even includes a wrapper around make to make this automatic:

```
compiledb make
```

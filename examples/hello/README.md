# Learn Tabula with a greeting app

Copy this directory into a temporary folder. The tiny `src/greet.js` function is
source evidence; Tabula never executes it. Read `.tabula/ENTRY.md`, then run:

```sh
tabula update .
tabula check .
tabula context . --files src/greet.js --max-bytes 4000 --json
```

Edit the greeting text in `src/greet.js` and run `tabula check .` again. It exits
1 and identifies the explanation that cites that file. Review the guide, update
its prose if needed, then run `tabula update .` and `tabula check .`.

An update records bytes; it does not prove correctness. To record an explicit
review, obtain the section ID from `tabula check . --json` and pass it to
`tabula update . --review SECTION`. Test results are separate caller assertions;
see the main CLI reference for `test-snapshot` and `import-test`.

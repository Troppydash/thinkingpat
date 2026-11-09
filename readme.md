# Thinkingpat
A limit texas hold'em solver.

## Backgroud

### Limit Rules
- who goes first: 
    - preflop: sb
    - flop/turn/river, bb
- can: call (0 or match), raise (>0), fold
- raise
    - preflop/flop, increments of 1bb
    - turn/river, increments of 2bb
    - max of 4 raises


### Combinatorics
- Cns assigns a dense index for a nCk choice
- Multiset is indexing an unordered set of perms, we use it to group equal size suits.
    - Index consists of getting underlying cns, sort, add [0,1,2,3], observe the resulting list forms a choice and apply cns
    - Size of multiset is choose(cns N + num sets - 1, num sets)

## Build
```bash
# generate and visualize groups in ./data
./generate/equity.sh

# cfr for version, output is in ./data/checkpoint/11_05
./generate/solve.sh 11_05

# productionalize binary in ./builds/2/thinkingpat
./generate/build.sh 2

# productionalize inference bin, writes checkpoint to ./builds/2/inference.bin
./generate/export.sh ./data ./data/checkpoint/11_05/20000000.bin 2

# make sure to bump the version!
echo '2' > ./generate/ver.txt
```

Important notes
- ./data items should be pure/imm, never change an existing version!
- ./builds/$ver for ver <= ver.txt are imm!

## Testing
```bash
# run arena between 2+ bots
cargo run -r -- "arena ./data 1000000 ./builds/1/thinkingpat:./builds/1/inference.bin ./builds/2/thinkingpat:./builds/2/inference.bin"
```

## Code
Modules
- `arena` to run 1v1s
- `bot` as a wrapper to a player
- `dist` for equity distribution calcs
- `game` for poker rules
- `group` for infoset groups
- `solver` cfr engine
- `ver` hacky version control

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
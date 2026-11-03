# ram usage calculator
preflop=20
flop=30
turn=30
river=5

mb="(1024*1024)"
echo "$(echo "$preflop * $flop * $turn * $river * 2^(4*4) * 24 / 8 / $mb" | bc)mb"
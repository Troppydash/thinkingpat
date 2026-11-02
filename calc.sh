# ram usage calculator
preflop=32
flop=20
turn=20
river=10

mb="(1024*1024)"
echo "$(echo "$preflop * $flop * $turn * $river * 2^(4*4) * 24 / 8 / $mb" | bc)mb"
#!/bin/zsh
# Offers two ways to run the typed command. The work happens only when a
# result is chosen (in do.sh), never while typing.
dir=${0:A:h}
cmd=$1
if [[ -z $cmd ]]; then
  print '{"items":[{"uid":"hint","title":"Type a shell command","action":{"type":"none"}}]}'
  exit 0
fi
j=${cmd//\\/\\\\}; j=${j//\"/\\\"}; j=${j//$'\t'/\\t}
d=${dir//\\/\\\\}; d=${d//\"/\\\"}
print -r -- "{\"items\":[
 {\"uid\":\"terminal\",\"title\":\"Run in Terminal: $j\",\"subtitle\":\"Opens a Terminal window\",\"icon\":{\"kind\":\"symbol\",\"value\":\"terminal\"},
  \"action\":{\"type\":\"run\",\"argv\":[\"/bin/zsh\",\"$d/do.sh\",\"terminal\",\"$j\"]}},
 {\"uid\":\"silent\",\"title\":\"Run silently: $j\",\"subtitle\":\"Copies the output and shows a notification\",\"icon\":{\"kind\":\"symbol\",\"value\":\"terminal.fill\"},
  \"action\":{\"type\":\"run\",\"argv\":[\"/bin/zsh\",\"$d/do.sh\",\"silent\",\"$j\"]}}
]}"

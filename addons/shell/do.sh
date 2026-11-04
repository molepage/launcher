#!/bin/zsh
mode=$1 cmd=$2
as=${cmd//\\/\\\\}; as=${as//\"/\\\"}
if [[ $mode == terminal ]]; then
  osascript -e "tell application \"Terminal\" to do script \"$as\"" -e 'tell application "Terminal" to activate'
else
  out=$(/bin/zsh -lc "$cmd" 2>&1); code=$?
  print -rn -- "$out" | pbcopy
  first=${${(f)out}[1]}; first=${first//\\/\\\\}; first=${first//\"/\\\"}
  osascript -e "display notification \"${first:0:120}\" with title \"Exit $code — output copied\""
fi

set -eu
find samples -name '*.wav' -print0 | xargs -0 -P "$(nproc)" -I{} bash -c '
  f="$1"
  rel="${f#samples/}"
  out="samples_ogg/${rel%.wav}.opus"
  mkdir -p "$(dirname "$out")"
  if [ -f "$out" ]; then
    exit 0
  fi
  echo "converting $f -> $out"
  ffmpeg -nostdin -nostats -loglevel error -hide_banner -i "$f" \
    -ac 1 -c:a libopus -b:a 24k -vbr on -application voip -compression_level 10 \
    "$out"
' _ {}

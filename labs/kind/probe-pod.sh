# shellcheck shell=bash
# Roda DENTRO do pod cliente (lido pelo bash do pod via stdin, ver probe.sh).
# Faz o que uma pessoa faria com dig e nc, sem nenhuma inteligência: é a visão
# de quem está do lado A. Argumentos: timeout e grupos de quatro valores
# (rótulo, nome, porta, IP de reserva). O IP de reserva só é usado quando o DNS
# falha, para separar "DNS quebrado" de "caminho quebrado".

t=$1
shift

timed_out() { (( $1 == 124 || $1 == 143 )); }
now_us() { local n=$EPOCHREALTIME; echo "${n//[!0-9]/}"; }

dns_check() {  # imprime "<estado> <ip>"
  local out ip
  out=$(dig +time="$t" +tries=1 +noall +comments +answer "$1" A 2>&1)
  ip=$(awk '$4 == "A" { print $5; exit }' <<<"$out")
  case $out in
    *"status: NOERROR"*)  if [[ -n $ip ]]; then echo "ok $ip"; else echo "nodata -"; fi ;;
    *"status: NXDOMAIN"*) echo "nxdomain -" ;;
    *"status: SERVFAIL"*) echo "servfail -" ;;
    *"status: REFUSED"*)  echo "rcode-refused -" ;;
    *"connection refused"*) echo "refused -" ;;
    *"timed out"*|*"no servers could be reached"*) echo "timeout -" ;;
    *) echo "error -" ;;
  esac
}

tcp_check() {  # imprime "<estado> <ms>"
  local err rc start ms
  start=$(now_us)
  # shellcheck disable=SC2016  # $1 e $2 expandem dentro do bash -c
  err=$(timeout "$t" bash -c 'exec 3<>"/dev/tcp/$1/$2"' _ "$1" "$2" 2>&1)
  rc=$?
  ms=$(( ($(now_us) - start) / 1000 ))
  if (( rc == 0 )); then echo "ok $ms"
  elif timed_out "$rc"; then echo "timeout $ms"
  else
    case $err in
      *refused*) echo "refused $ms" ;;
      *"No route"*|*nreachable*) echo "unreachable $ms" ;;
      *) echo "error $ms" ;;
    esac
  fi
}

# Postgres: manda um SSLRequest e espera um byte ('S' ou 'N'). Prova que o
# processo do banco está respondendo, não só o kernel do destino.
app_check() {
  local reply rc
  # shellcheck disable=SC2016  # $1 e $2 expandem dentro do bash -c
  reply=$(timeout "$t" bash -c 'exec 3<>"/dev/tcp/$1/$2" || exit 2
    printf "\x00\x00\x00\x08\x04\xd2\x16\x2f" >&3
    IFS= read -r -n1 -d "" c <&3
    printf %s "$c"' _ "$1" "$2" 2>/dev/null)
  rc=$?
  case $reply in
    N|S) echo ok ;;
    *) if timed_out "$rc"; then echo timeout; else echo closed; fi ;;
  esac
}

while (( $# >= 4 )); do
  label=$1 name=$2 port=$3 fallback=$4
  shift 4
  read -r dns ip <<<"$(dns_check "$name")"
  if [[ $dns == ok ]]; then target=$ip via=dns; else target=$fallback via=reserva; fi
  read -r tcp ms <<<"$(tcp_check "$target" "$port")"
  app=-
  [[ $tcp == ok ]] && app=$(app_check "$target" "$port")
  printf '%-4s dns=%-9s tcp=%-8s app=%-8s | %s:%s ip=%s via=%s tcp_ms=%s\n' \
    "$label" "$dns" "$tcp" "$app" "$name" "$port" "$target" "$via" "$ms"
done

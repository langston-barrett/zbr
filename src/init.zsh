export RUST_BACKTRACE=1

function zbr-hint() {
  if [[ -n ${BUFFER% } ]]; then
    out=$(printf "\n" && env RUST_BACKTRACE=1 zbr hint --max 5 "${ZBR_CONF}" -- "${BUFFER}")
    if [[ -n $out ]]; then
      zle -M "${out}"
    else
      zle -M ""
    fi
  fi
}

function zbr-expand() {
  out=$(env RUST_BACKTRACE=1 zbr expand "${ZBR_CONF}" -- "${LBUFFER}" "${RBUFFER}")
  if [ "${?}" -eq 0 ] && [ -n "${out}" ]; then
    BUFFER=${out}
    CURSOR=${#BUFFER}
  fi
}

function zbr-space() {
  if [[ "${BUFFER% }" == "${BUFFER}" ]] || [[ "${CURSOR}" != "${#BUFFER}" ]]; then
    zbr-expand
  fi
  if [[ ${BUFFER% } == ${BUFFER} ]]; then
    zle .self-insert
  fi
  zbr-hint
}

function zbr-ret() {
  zle zbr-expand
  zle accept-line
}

zle -N zbr-expand

# space
zle -N zbr-space
bindkey -M emacs " " zbr-space
bindkey -M viins " " zbr-space

# control-space is a normal space
bindkey -M emacs "^ " magic-space
bindkey -M viins "^ " magic-space

# ret
zle -N zbr-ret
bindkey -M emacs "^M" zbr-ret
bindkey -M viins "^M" zbr-ret

function zle-line-pre-redraw() {
  zbr-hint
}
zle -N zle-line-pre-redraw

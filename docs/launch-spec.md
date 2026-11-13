# Especificação de lançamento

A especificação de lançamento é o contrato entre quem compõe a política (o cliente `sandmux` e, no futuro, o daemon) e o lançador `sandmux-run`. O lançador não conhece harnesses nem política: monta exatamente o que a especificação descreve e executa o comando.

## Como é passada

O cliente grava a especificação em JSON num descritor de arquivo herdado e executa:

```
sandmux-run --spec-fd <N>
```

O lançador lê o descritor até o fim, fecha e valida o conteúdo antes de criar qualquer processo.

## Códigos de saída

| Código | Significado |
|---|---|
| O do comando | O comando rodou; o código é o dele, ou 128 mais o número do sinal que o encerrou |
| 125 | A especificação foi recusada ou o confinamento não pôde ser montado. O comando não rodou |
| 127 | O confinamento foi montado, mas o comando não pôde ser executado (não encontrado, sem permissão) |

## Regras de leitura

- O campo `version` é conferido antes de todos os outros. Uma versão diferente de `1` é recusada com uma mensagem que mostra a versão recebida e a suportada.
- Um campo desconhecido, em qualquer nível, faz a leitura falhar.
- Todos os caminhos são absolutos e não podem conter `..`.
- `command` não pode ser vazio. Quando o primeiro item não contém `/`, ele é procurado no `PATH` de `env`.

## Campos

| Campo | Conteúdo |
|---|---|
| `version` | Versão do formato: `1` |
| `mounts` | Lista de montagens, aplicadas em ordem sobre uma raiz nova e vazia |
| `landlock.read` | Caminhos com leitura e execução |
| `landlock.write` | Caminhos com todos os acessos |
| `seccomp.deny_syscalls` | Chamadas de sistema que devolvem erro de permissão |
| `seccomp.deny_ioctls` | Comandos de `ioctl` que devolvem erro de permissão (`TIOCSTI`, `TIOCLINUX`) |
| `env` | Ambiente completo do comando, como lista de `{name, value}`. Nada é herdado |
| `command` | Programa e argumentos |
| `cwd` | Pasta em que o comando começa, dentro do sandbox |

## Montagens

Em todas, `dest` é o caminho dentro do sandbox. `source`, `lower` e os caminhos de camada são caminhos do host. Os pontos de montagem são criados quando faltam, componente a componente, e um link simbólico em qualquer componente do destino faz o lançamento falhar.

| `type` | Campos | Efeito |
|---|---|---|
| `tmpfs` | `dest`, `mode` | Sistema de arquivos em memória, novo e privado. `mode` é numérico (por exemplo `1023` para `0o1777`) |
| `bind` | `source`, `dest`, `writable` | Pasta ou arquivo do host, recursivamente; em somente leitura quando `writable` é falso |
| `device` | `source`, `dest` | Nó de dispositivo do host |
| `overlay` | `lower`, `dest`, `layer` | Pasta do host por baixo, em somente leitura, com uma camada de escrita por cima |
| `cover` | `dest` | O caminho passa a aparecer vazio e em somente leitura. Se ele não existir, ou se for um link simbólico, nada é feito |
| `file` | `dest`, `contents`, `mode` | Arquivo gerado, em somente leitura |
| `symlink` | `dest`, `target` | Link simbólico criado na raiz nova |
| `proc` | `dest` | `/proc` novo, do pid namespace do sandbox |
| `devpts` | `dest` | Instância nova de pseudoterminais |

A camada de um `overlay` é `{"kind": "persistent", "upper": ..., "work": ...}`, guardada no host e mantida entre execuções, ou `{"kind": "temporary"}`, guardada em memória e descartada quando o sandbox termina.

Depois de todas as montagens, a raiz nova passa a ser somente leitura.

## Exemplo

O exemplo abaixo é conferido por um teste do projeto (`crates/sandmux/tests/docs.rs`).

```json
{
  "version": 1,
  "mounts": [
    {"type": "bind", "source": "/usr", "dest": "/usr", "writable": false},
    {"type": "symlink", "dest": "/bin", "target": "usr/bin"},
    {"type": "symlink", "dest": "/lib", "target": "usr/lib"},
    {"type": "symlink", "dest": "/lib64", "target": "usr/lib64"},
    {"type": "bind", "source": "/etc", "dest": "/etc", "writable": false},
    {"type": "cover", "dest": "/etc/shadow"},
    {"type": "file", "dest": "/etc/passwd", "contents": "ana:x:1000:1000::/home/ana:/bin/bash\n", "mode": 420},
    {"type": "proc", "dest": "/proc"},
    {"type": "tmpfs", "dest": "/dev", "mode": 493},
    {"type": "device", "source": "/dev/null", "dest": "/dev/null"},
    {"type": "devpts", "dest": "/dev/pts"},
    {"type": "tmpfs", "dest": "/tmp", "mode": 1023},
    {"type": "bind", "source": "/home/ana/.local/share/sandmux/homes/shell", "dest": "/home/ana", "writable": true},
    {"type": "overlay", "lower": "/home/ana/.cargo", "dest": "/home/ana/.cargo",
     "layer": {"kind": "persistent",
               "upper": "/home/ana/.local/share/sandmux/layers/shell/.cargo/upper",
               "work": "/home/ana/.local/share/sandmux/layers/shell/.cargo/work"}},
    {"type": "bind", "source": "/home/ana/projetos/app", "dest": "/home/ana/projetos/app", "writable": true}
  ],
  "landlock": {
    "read": ["/"],
    "write": ["/home/ana", "/home/ana/projetos/app", "/tmp", "/dev", "/proc"]
  },
  "seccomp": {
    "deny_syscalls": ["mount", "umount2", "pivot_root", "bpf", "keyctl", "io_uring_setup"],
    "deny_ioctls": ["TIOCSTI"]
  },
  "env": [
    {"name": "HOME", "value": "/home/ana"},
    {"name": "PATH", "value": "/home/ana/.cargo/bin:/usr/bin:/bin"}
  ],
  "command": ["bash"],
  "cwd": "/home/ana/projetos/app"
}
```

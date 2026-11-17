## Why

Depois do change 69, o que protege os arquivos da pasta contra um contêiner do Docker privado são montagens: as coberturas dos segredos e do `file-deny`, o somente-leitura de `.git/hooks` e `.git/config`. Um contêiner com `--privileged` tem `CAP_SYS_ADMIN` no user namespace onde a visão foi montada e desfaz essas montagens. O caso mais sério é o `.git`: um hook gravado assim roda no host quando o usuário usa o git. A criação de `.git/commondir`, negada ao agente desde o change 16, também fica aberta a um contêiner.

Decisões do usuário (2026-11-17): fechar o caso privilegiado só pelo eBPF, sem um change de trancar a visão; numa máquina sem eBPF, o Docker privado sobe com o nível do change 69 e um aviso; os testes que exigem root rodam por um script fixo, com `sudo` sem senha temporário.

## What Changes

- **Auxiliar root `sandmux-helper`:** serviço do sistema, ativado por socket, que carrega programas BPF LSM e os prende ao cgroup do Docker de uma pasta.
  - Só atende o cgroup que pertence ao usuário que pede. Uma regra só restringe processos desse cgroup.
  - Os programas e as marcas duram enquanto o daemon mantiver a conexão aberta.
- **Decisão pelo arquivo, e não pelo caminho:** quando o Docker de uma pasta sobe, o daemon manda ao auxiliar os arquivos que a visão protege. O kernel passa a decidir pelo inode, então desmontar uma cobertura, tirar um somente-leitura, renomear ou criar um link não muda a decisão.
  - **Negados à leitura e à escrita:** os segredos e o `file-deny` cobertos pelo change 69 e as coberturas de `/etc`. Um caminho negado que é pasta nega tudo o que está dentro dela.
  - **Protegidos contra escrita:** `.git/hooks`, `.git/config` e `.git/config.worktree` de cada diretório git da pasta (com os submódulos). Ficam negados escrever, criar dentro, apagar, renomear, mudar atributos e fazer link.
  - **Nomes negados nos diretórios git:** criar `commondir`, `hooks`, `config` ou `config.worktree`.
- **Ativação automática, com recuo:** com o auxiliar instalado e o BPF LSM ativo no kernel, o Docker de cada pasta sobe confinado. Sem eles, sobe só com o endurecimento do change 69. O `sandmux doctor` e o `sandmux stats` mostram qual dos dois vale e por quê.
  - Se o auxiliar cair com o Docker rodando, o daemon para o Docker da pasta.
- **Instalação:** o `sandmux setup` instala as unidades do auxiliar com `sudo`, como já faz com o perfil do AppArmor. Os pacotes `.deb`, `.rpm` e do AUR já trazem as unidades.
  - O setup recusa uma unidade que aponte para um binário que o usuário pode alterar, porque isso daria root a ele.
- **Testes com root:** `scripts/check-root.sh`, fora do `check.sh`. Ele sobe o auxiliar como root num socket temporário e roda os testes do Docker confinado como o usuário.

Fora deste change, para um change seguinte (71, provisório):

- o registro de arquivos dos contêineres;
- os segredos criados com o Docker já rodando;
- a mudança ao vivo da política.

Decisão do agente: o change 70 do roadmap foi dividido para caber num change. Também não entram regras sobre o que pode ser montado: com a decisão pelo inode, uma montagem não abre caminho até um arquivo protegido.

## Capabilities

### New Capabilities
- `root-helper`: o auxiliar root. Cobre o que ele aceita fazer, quem pode pedir, os limites por usuário, quanto tempo as regras duram e o que acontece quando a conexão cai.

### Modified Capabilities
- `container-engine`: o confinamento dos arquivos do Docker privado por BPF LSM, a ativação com recuo e o aviso.
- `installation`: o binário `sandmux-helper`, as unidades do systemd do sistema, o `sandmux setup` e os pacotes.

## Impact

- Novo crate `crates/sandmux-ebpf`, sem dependência nova de toolchain (sem clang e sem nightly):
  - leitura do BTF do kernel (`/sys/kernel/btf/vmlinux`), com os ids dos ganchos e os deslocamentos dos campos;
  - montagem das instruções BPF em Rust;
  - a chamada `bpf()` para mapas, programas e links.
- Novo binário `sandmux-helper` (`crates/sandmux/src/bin/`), com um protocolo de linhas JSON e descritores passados por `SCM_RIGHTS`.
- `crates/sandmux/src/daemon/docker.rs`: liga o auxiliar depois da subida do Docker e antes de entregar o socket; para o Docker se a conexão cair.
- `crates/sandmux-policy`: a lista de marcas tirada da visão do Docker (coberturas, binds em somente leitura e diretórios git).
- `crates/sandmux/src/setup.rs` e `doctor.rs`: a instalação das unidades e a verificação do confinamento.
- Protocolo entre o cliente e o daemon: o `DockerStats` ganha o estado do confinamento. A versão do protocolo muda.
- `packaging/`: as unidades `sandmux-helper.socket` e `sandmux-helper.service`; `install.sh`, `dist.sh`, `.deb`, `.rpm` e o AUR passam a levar o binário novo.
- `scripts/check-root.sh` e os testes com root em `crates/sandmux/tests/`.
- `docs/docker.md`, `docs/install.md`, um `docs/helper.md` novo e os pares em inglês.

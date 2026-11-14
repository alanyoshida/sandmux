## 1. Binários e configuração

- [x] 1.1 Build estático com musl: `ioctl` portável no filtro de arquivos e no `sandmux-probe`, e a receita de build com `CC_x86_64_unknown_linux_musl`
- [x] 1.2 `sandmux setup` (`--no-sudo`, `--no-daemon`): perfil do AppArmor com o caminho do `sandmux-run` instalado, comparação com o carregado, `sudo` com pergunta, daemon e diagnóstico, com testes

## 2. Distribuição

- [x] 2.1 `packaging/install.sh`: privilégio, destino, origem local ou da release com SHA-256, `sandmux setup`, `--uninstall`
- [x] 2.2 `scripts/dist.sh`: tarballs e somas, `.deb` com `postinst` e `postrm`, `.rpm` por `rpmbuild`, `PKGBUILD` e `.SRCINFO`, fórmula do Homebrew
- [x] 2.3 `.github/workflows/release.yml` numa tag `v*`

## 3. Verificação, documentação e release

- [x] 3.1 Teste do `install.sh` sem privilégio nesta máquina, num HOME temporário
- [x] 3.2 `scripts/distro-test.sh` com Debian 13, Fedora atual e Arch em contêineres, sob o vigia de memória
- [x] 3.3 `docs/install.md`, README e teste de documentação
- [ ] 3.4 Publicação da release `v0.1.0` no GitHub, depois da confirmação do usuário sobre os commits e a tag

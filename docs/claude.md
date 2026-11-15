# Claude Code

> English: [en/claude.md](en/claude.md)

O preset `claude` roda o [Claude Code](https://code.claude.com) instalado na sua máquina dentro do sandbox, como o opencode e o hermes.

```sh
sandmux run claude
```

## Instalação

O sandmux usa a instalação nativa do host: o link `~/.local/bin/claude`, que aponta para uma versão em `~/.local/share/claude/versions`. Sem ela, o `run` recusa e mostra o comando de instalar:

```sh
curl -fsSL https://claude.ai/install.sh | bash
```

A instalação entra em somente leitura, e a atualização automática fica desligada (`DISABLE_AUTOUPDATER=1`). Para atualizar, rode `claude update` fora do sandbox.

## Importação

Na primeira execução, o sandmux pergunta se importa a configuração que você já tem. Ele copia para o HOME sintético:

- `~/.claude.json`;
- de `~/.claude`: `settings.json`, `CLAUDE.md`, `agents/`, `commands/`, `skills/`, `hooks/`, `plugins/` e `output-styles/`.

Ficam de fora:

- `~/.claude/.credentials.json`, com o token da sua assinatura (veja abaixo);
- as conversas (`projects/`), o histórico e os caches.

## Login pela assinatura

O token da assinatura (OAuth) nunca entra no sandbox. Ele não é importado, por dois motivos:

- **Rotação:** a Anthropic troca o token de atualização a cada renovação. Se o sandbox e o Claude Code de fora do sandbox usassem o mesmo login, um derrubaria o outro.
- **Exposição:** o agente veria o token da sua conta.

Por isso, o login é feito uma vez dentro do sandmux, com `/login` no Claude Code:

1. Escolha a conta com assinatura. O Claude Code mostra um endereço.
2. Abra o endereço no navegador do host e autorize.
3. A página mostra um código. Cole o código no Claude Code (o retorno automático em `localhost` não alcança o sandbox).

O gateway decifra `platform.claude.com` e `api.anthropic.com` nas abas do Claude Code. Na resposta do login e de cada renovação (`/v1/oauth/token`), o gateway:

1. guarda o token de acesso e o de atualização no cofre, como `claude:access` e `claude:refresh`;
2. entrega ao Claude Code falsos com o mesmo prefixo (`sk-ant-oat01-sandmux…`);
3. troca, nos pedidos seguintes, o falso pelo real: nos cabeçalhos para `api.anthropic.com` e no corpo da renovação.

Os falsos são estáveis: depois de uma renovação, o Claude Code recebe os mesmos falsos, e o cofre passa a ter os reais novos.

O login pela "Console" (cobrança por uso da API) cria uma chave de API (`/api/oauth/claude_cli/create_api_key`). Ela é capturada do mesmo jeito, como `claude:api-key`.

**Cofre:** o cofre precisa existir e estar desbloqueado no login e nas renovações. Sem cofre que aceite, a resposta é recusada ao Claude Code, e nenhum token real chega ao sandbox. Desbloqueie (`sandmux vault unlock`, ou a seção Cofre das configurações da TUI) e repita o `/login`.

**Chaves dos outros provedores:** o Claude Code não recebe as chaves falsas dos provedores do cofre (`ANTHROPIC_API_KEY` e afins). Com uma `ANTHROPIC_API_KEY`, ele trocaria a assinatura pela cobrança da API.

## Rede

`platform.claude.com` está no catálogo de inferência, e por isso o modo só inferência deixa a renovação passar. O primeiro login precisa da rede aberta (o padrão) ou de `platform.claude.com` liberado.

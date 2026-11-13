# sandmux

Multiplexador de terminal em Rust com ratatui para rodar harnesses de agente (opencode, hermes) dentro de um sandbox de segurança. O projeto usa OpenSpec para planejar cada mudança.

## Roadmap

`openspec/roadmap.md` é o registro de todas as decisões do projeto e o plano sequencial de etapas e changes. Leia antes de propor, validar ou implementar qualquer change.

**Mantenha o roadmap atualizado.** Atualize `openspec/roadmap.md` na mesma sessão sempre que:

- um artefato do OpenSpec for criado ou alterado (`proposal.md`, `design.md`, specs ou `tasks.md`);
- um change for validado (`openspec validate`), implementado ou arquivado;
- uma decisão registrada mudar, ou uma decisão em aberto for tomada.

O que atualizar em cada caso:

- a situação do change na tabela da etapa (seção 4), e o nome dele se deixou de ser provisório;
- as decisões (seção 2), quando o artefato decidir algo novo ou diferente;
- as verificações, os riscos e as decisões ainda não tomadas (seção 6);
- o rastreio dos pedidos originais (seção 7), quando um pedido mudar de change.

Se um artefato do OpenSpec e o roadmap divergirem, o artefato vale e o roadmap é corrigido. Nada do que foi discutido deve se perder: prefira mover um item para "Depois da v1" ou para os pontos em aberto a apagá-lo.

## Idioma

Os artefatos do OpenSpec e a documentação são escritos em português do Brasil, mantendo em inglês os termos técnicos (seccomp, Landlock, allowlist, denylist) e os títulos de seção que os templates do OpenSpec exigem.

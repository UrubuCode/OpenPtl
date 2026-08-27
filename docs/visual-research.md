# Pesquisa visual para o OpenPtl

## Referências consultadas

1. [egui — documentação oficial](https://docs.rs/egui/latest/egui/)
2. [Nielsen Norman Group — Designing Empty States in Complex Applications](https://www.nngroup.com/articles/empty-state-interface-design/)

## Decisões derivadas

A documentação do egui confirma que a composição nativa deve ser baseada em `CentralPanel`, `SidePanel`, `TopBottomPanel`, `Window`, `ScrollArea`, `Button`, `TextEdit`, `Ui::horizontal`, `Ui::columns` e um `Style` centralizado. A interface deve manter o estado no modelo da aplicação e usar os containers para organizar hierarquia, tamanhos e fluxo de interação.

A referência da Nielsen Norman Group destaca que estados vazios não devem ser áreas simplesmente em branco. Eles devem comunicar o estado do sistema, ensinar o próximo passo e oferecer uma ação direta. Isso será aplicado às telas de conexões, keychain e workspace com mensagens específicas, CTA contextual e distinção entre vazio, carregamento e erro.

## Direção visual

O OpenPtl seguirá uma linguagem de desktop operacional: fundo escuro neutro, superfícies elevadas em camadas, azul como ação primária, verde para estado seguro/conectado, amarelo para atenção e vermelho apenas para ações destrutivas. A navegação lateral será persistente, o cabeçalho exibirá contexto e sessão, e os conteúdos serão organizados em cards com títulos, descrições e ações agrupadas.

Os modais serão reservados para decisões que interrompem o fluxo ou exigem confirmação, como exclusão de credenciais, exclusão de perfis e aprovação de fingerprint SSH. Ações comuns devem permanecer inline. O feedback de operações deverá aparecer com status visível e linguagem específica, sem depender de mudanças silenciosas na tela.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Relógio lógico híbrido.
///
/// O `wall` continua legível para o usuário, mas a ordem nunca depende só
/// dele: dois aparelhos com relógios diferentes ainda produzem uma sequência
/// coerente porque quem recebe adianta o próprio relógio para o maior valor
/// visto. Ordenar por data de criação no Drive não serviria — ela marca quando
/// o arquivo subiu, não quando a mudança aconteceu, e um aparelho que ficou
/// offline sobreescreveria edições mais novas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct Hlc {
    pub wall: i64,
    pub counter: u32,
}

/// Marca completa de uma operação. O `device` é o desempate final: sem ele,
/// dois aparelhos que gerassem o mesmo `Hlc` aplicariam ordens diferentes e o
/// estado divergiria em silêncio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Stamp {
    pub hlc: Hlc,
    pub device: Uuid,
}

impl Stamp {
    pub fn new(hlc: Hlc, device: Uuid) -> Self {
        Self { hlc, device }
    }
}

/// Gerador do relógio, guardado junto com o estado local.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct HlcClock {
    last: Hlc,
}

impl HlcClock {
    /// Marca uma operação local.
    pub fn tick(&mut self, now: i64) -> Hlc {
        if now > self.last.wall {
            self.last = Hlc {
                wall: now,
                counter: 0,
            };
        } else {
            self.last.counter = self.last.counter.saturating_add(1);
        }
        self.last
    }

    /// Absorve a marca de um lote recebido. Chamar isto ao aplicar mutações
    /// remotas é o que garante causalidade: uma edição feita depois de ver a
    /// mudança do outro aparelho recebe marca maior, mesmo com o relógio de
    /// parede atrasado.
    pub fn observe(&mut self, remote: Hlc, now: i64) -> Hlc {
        let wall = now.max(self.last.wall).max(remote.wall);
        let counter = if wall == self.last.wall && wall == remote.wall {
            self.last.counter.max(remote.counter).saturating_add(1)
        } else if wall == self.last.wall {
            self.last.counter.saturating_add(1)
        } else if wall == remote.wall {
            remote.counter.saturating_add(1)
        } else {
            0
        };
        self.last = Hlc { wall, counter };
        self.last
    }

    pub fn last(&self) -> Hlc {
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frozen_clock_still_advances_the_counter() {
        let mut clock = HlcClock::default();
        let first = clock.tick(100);
        let second = clock.tick(100);
        assert!(second > first);
        assert_eq!(second.wall, 100);
        assert_eq!(second.counter, 1);
    }

    #[test]
    fn a_clock_that_goes_backwards_never_produces_an_older_stamp() {
        let mut clock = HlcClock::default();
        let first = clock.tick(500);
        let second = clock.tick(10);
        assert!(second > first);
        assert_eq!(second.wall, 500);
    }

    #[test]
    fn observing_a_future_remote_stamp_pulls_the_local_clock_forward() {
        let mut clock = HlcClock::default();
        clock.tick(100);
        let observed = clock.observe(
            Hlc {
                wall: 9_000,
                counter: 3,
            },
            120,
        );
        assert_eq!(observed.wall, 9_000);
        assert!(observed.counter > 3);

        // Uma edição local posterior tem de ficar depois do que foi observado.
        let local = clock.tick(130);
        assert!(local > observed);
    }

    #[test]
    fn the_device_breaks_ties_between_identical_clocks() {
        let hlc = Hlc {
            wall: 7,
            counter: 0,
        };
        let low = Stamp::new(hlc, Uuid::from_u128(1));
        let high = Stamp::new(hlc, Uuid::from_u128(2));
        assert!(high > low);
    }
}

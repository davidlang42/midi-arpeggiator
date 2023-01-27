use std::collections::HashMap;
use crate::arpeggio::{Player, Arpeggio};

pub mod timed;

pub trait Arpeggiator {
    fn listen(&mut self);
    fn stop_arpeggios(&mut self);
}

fn drain_and_stop<N, A: Arpeggio>(arpeggios: &mut HashMap<N, Player<A>>) -> Vec<Player<A>> {
    let mut players = Vec::new();
    for (_, mut player) in arpeggios.drain() {
        player.stop();
        players.push(player);
    }
    players
}

fn drain_and_wait_for_stop<N, A: Arpeggio>(arpeggios: &mut HashMap<N, Player<A>>) -> Vec<A> {
    let mut results = Vec::new();
    for player in drain_and_stop(arpeggios) {
        results.push(player.ensure_stopped().unwrap()); //TODO handle error
    }
    results
}
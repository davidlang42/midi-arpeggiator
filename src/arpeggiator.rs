use std::collections::HashMap;
use crate::arpeggio::GenericPlayer;

pub mod timed;

pub trait Arpeggiator {
    fn listen(&mut self);
    fn stop_arpeggios(&mut self);
}

fn drain_and_stop<N, P: GenericPlayer>(arpeggios: &mut HashMap<N, P>) -> Vec<P> {
    let mut players = Vec::new();
    for (_, mut player) in arpeggios.drain() {
        player.stop();
        players.push(player);
    }
    players
}

fn drain_and_wait_for_stop<N, P: GenericPlayer>(arpeggios: &mut HashMap<N, P>) {
    for player in drain_and_stop(arpeggios) {
        player.ensure_stopped().unwrap(); //TODO handle error
    }
}
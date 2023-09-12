use std::str::FromStr;

use bitris::macros::piece;
use bitris::prelude::*;

fn main() {
    use Orientation::*;
    use Shape::*;

    let board64: Board64 = Board64::from_str(
        "
            ..........
            ......####
            .....#####
            ....######
            ...#######
            ..########
            .#########
        ",
    )
    .expect("Failed to create a board");

    let spawn = piece!(IN).with(cc(4, 20)).to_bl_placement();

    // Specify the rotation system and a drop type. The default is selected SRS & softdrop.
    // You can specify your own rotation system.
    let move_rules = MoveRules::default();
    // OR `let move_rules = MoveRules::srs(AllowMove::Softdrop);`

    let all_moves = move_rules.generate_all_moves(board64, spawn);
    assert_eq!(all_moves.len(), 34);

    let minimized_moves = move_rules.generate_minimized_moves(board64, spawn);
    assert_eq!(minimized_moves.len(), 17);

    // The result includes both orientations that have the same form.
    assert!(all_moves.contains(&Piece::new(I, North).with(bl(0, 3))));
    assert!(all_moves.contains(&Piece::new(I, South).with(bl(0, 3))));

    // The result includes one orientation that has the same form.
    assert!(minimized_moves.contains(&Piece::new(I, North).with(bl(0, 3))));
    assert!(!minimized_moves.contains(&Piece::new(I, South).with(bl(0, 3))));
}

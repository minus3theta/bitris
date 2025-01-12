/** This file is for internal */

pub(crate) mod moves64 {
    use std::ops::{Not, Shl, Shr};

    use crate::boards::Board;
    use crate::coordinates::{bl, BlPosition, Offset};
    use crate::pieces::{Orientation, Shape};
    use crate::placements::BlPlacement;
    use crate::{GenerateInstruction, Rotate, Rotation, RotationSystem, With};

    // The type of boards used within this module.
    type Type = u64;
    type ThisBoard = Board<Type>;

    const TYPE_MAX: Type = !0;

    // Locations where the block does not exist are represented by 1.
    #[derive(Debug)]
    pub struct FreeBoard {
        cols: [Type; 10],
    }

    impl FreeBoard {
        #[inline]
        pub fn from(board: &ThisBoard) -> FreeBoard {
            Self {
                cols: board.cols.map(|col| !col),
            }
        }
    }

    // The position where there is space to place a piece is represented by 1.
    // The flags are aggregated to the position that corresponds to Bottom-Left.
    #[derive(Copy, Clone, Debug)]
    pub struct FreePieceBoard {
        cols: [Type; 10],
    }

    impl FreePieceBoard {
        // Returns a new board, all initialized with non-free.
        #[inline]
        const fn non_free() -> Self {
            Self {
                cols: [TYPE_MAX; 10],
            }
        }
    }

    // It holds `FreePieceBoard` for each orientation of a shape.
    #[derive(Copy, Clone, Debug)]
    pub struct FreePieceBoards {
        boards: [FreePieceBoard; 4],
    }

    impl FreePieceBoards {}

    impl FreePieceBoards {
        // Returns new boards, all initialized with non-free.
        #[inline]
        pub const fn non_free() -> Self {
            Self {
                boards: [FreePieceBoard::non_free(); 4],
            }
        }

        // Return new boards initialized according to the piece.
        #[inline]
        pub fn new_according_to(shape: Shape, free_board: &FreeBoard) -> Self {
            let mut dest = Self::non_free();
            for piece in shape.all_pieces_iter() {
                let piece_blocks = piece.to_piece_blocks();
                for offset in piece_blocks.offsets {
                    Self::keep_if_offset_dest_is_free(
                        &mut dest.boards[piece.orientation as usize],
                        offset - piece_blocks.bottom_left,
                        free_board,
                    );
                }
            }
            dest
        }

        // When a block to which the offset destination is free(1), it keeps its bit.
        #[inline(always)]
        fn keep_if_offset_dest_is_free(
            free_piece_board: &mut FreePieceBoard,
            offset: Offset,
            free_board: &FreeBoard,
        ) {
            debug_assert!(0 <= offset.dy);

            for x in 0..10 {
                let offset_x = x as i32 + offset.dx;
                if (0..10).contains(&offset_x) {
                    // All `free_piece_board.cols` bits are initialized as 1.
                    // Then, if all four block offsets are free, it is determined that there is space to place the piece.
                    free_piece_board.cols[x] &= free_board.cols[offset_x as usize] >> offset.dy;
                } else {
                    // If the offset destination is outside the board, it cannot be placed.
                    free_piece_board.cols[x] = 0;
                }
            }
        }

        #[inline]
        pub fn is_free(&self, orientation: Orientation, position: BlPosition) -> bool {
            0 < (self.boards[orientation as usize].cols[position.lx as usize] & 1 << position.by)
        }
    }

    // The position that the piece can reach is represented by 1.
    // The flags are aggregated to the position that corresponds to Bottom-Left.
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub struct ReachablePieceBoard {
        cols: [Type; 10],
    }

    impl ReachablePieceBoard {
        // Remove positions where `other` is on.
        #[inline]
        pub fn remove(&mut self, other: &ReachablePieceBoard) {
            for x in 0..10 {
                self.cols[x] &= !other.cols[x];
            }
        }

        // Merge positions where `other` is on.
        #[inline]
        pub fn merge(&mut self, other: &ReachablePieceBoard) {
            for x in 0..10 {
                self.cols[x] |= other.cols[x];
            }
        }

        // Returns true if a flag at position is on.
        #[inline]
        pub fn can_reach(&self, position: BlPosition) -> bool {
            0 < (self.cols[position.lx as usize] & 1 << position.by)
        }
    }

    impl ReachablePieceBoard {
        // Returns a new board, all initialized with non-reach.
        #[inline]
        pub const fn non_reach() -> Self {
            Self { cols: [0; 10] }
        }

        #[inline]
        pub fn mark_with_reached(&mut self, position: BlPosition) {
            self.cols[position.lx as usize] |= 1 << position.by
        }

        #[inline]
        pub fn is_blank(&self) -> bool {
            self.cols.iter().all(|it| *it == 0)
        }

        #[inline]
        pub fn count_ones(&self) -> u32 {
            self.cols.iter().map(|col| col.count_ones()).sum::<u32>()
        }
    }

    // It holds `ReachablePieceBoard` for each orientation of a shape.
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub struct ReachablePieceBoards {
        boards: [ReachablePieceBoard; 4],
    }

    impl ReachablePieceBoards {
        // Returns new boards, all initialized with non-reach.
        #[inline]
        pub fn non_reach() -> Self {
            Self {
                boards: [ReachablePieceBoard::non_reach(); 4],
            }
        }

        // Mark the spawn position and the position rotated N times from the spawn position.
        #[inline]
        pub fn mark_spawn_and_its_post_rotations(
            &mut self,
            spawn: &BlPlacement,
            rotation_times: u32,
            free_piece_boards: &FreePieceBoards,
            rotation_system: &impl RotationSystem,
        ) {
            self.mark_with_reached(spawn.piece.orientation, spawn.position);

            for rotation in [Rotation::Cw, Rotation::Ccw] {
                self.mark_spawn_and_its_post_rotations_with_rotation(
                    spawn,
                    rotation,
                    rotation_times,
                    free_piece_boards,
                    rotation_system,
                );
            }
        }

        // (private common process for `mark_spawn_and_its_post_rotations`)
        fn mark_spawn_and_its_post_rotations_with_rotation(
            &mut self,
            spawn: &BlPlacement,
            rotation: Rotation,
            rotation_times: u32,
            free_piece_boards: &FreePieceBoards,
            rotation_system: &impl RotationSystem,
        ) {
            let mut from = *spawn;
            for _ in 0..rotation_times {
                let to = from.rotate(rotation);

                let mut next_from: Option<BlPlacement> = None;
                for kick in rotation_system.iter_kicks(from.piece, rotation) {
                    let moved = to + kick.offset;
                    if free_piece_boards.is_free(to.piece.orientation, moved.position) {
                        next_from = Some(moved);
                        break;
                    }
                }

                if let Some(placement) = next_from {
                    self.mark_with_reached(placement.piece.orientation, placement.position);
                    from = placement;
                } else {
                    return;
                }
            }
        }

        // Mark a position that can be reached.
        #[inline(always)]
        fn mark_with_reached(&mut self, orientation: Orientation, position: BlPosition) {
            self.boards[orientation as usize].mark_with_reached(position);
        }

        // From left to right, update the reachable position.
        // The number of updates is always fixed at 9.
        #[inline]
        fn update_by_moving_right(&mut self, free: &FreePieceBoards) {
            for orientation_index in 0..4 {
                for x in 0..9 {
                    self.boards[orientation_index].cols[x + 1] |= self.boards[orientation_index]
                        .cols[x]
                        & free.boards[orientation_index].cols[x + 1];
                }
            }
        }

        // From right to left, update the reachable position.
        // The number of updates is always fixed at 9.
        #[inline]
        fn update_by_moving_left(&mut self, free: &FreePieceBoards) {
            for orientation_index in 0..4 {
                for x in (1..10).rev() {
                    self.boards[orientation_index].cols[x - 1] |= self.boards[orientation_index]
                        .cols[x]
                        & free.boards[orientation_index].cols[x - 1];
                }
            }
        }

        // From up to down, update the reachable position.
        // One operation is relatively heavy and is processed in each contiguous block of cols, so there is no parallelism.
        // Instead, O(1) can be used to determine the harddrop destination.
        // Therefore, it's effective in the early stage of the search, when there are few bits in col and many moves can be made with Harddrop.
        #[inline]
        fn update_by_moving_harddrop(&mut self, free_piece_boards: &FreePieceBoards) {
            for orientation_index in 0..4 {
                for x in 0..10 {
                    let mut col = self.boards[orientation_index].cols[x];
                    while 0 < col {
                        let y = col.trailing_zeros();
                        let mask_drop_from_y = ((1 as Type) << y) - 1;
                        let blocks_on_board = (!free_piece_boards.boards[orientation_index].cols
                            [x])
                            & mask_drop_from_y;
                        let top_y = 64 - blocks_on_board.leading_zeros();
                        let mask_unreachable = ((1 as Type) << top_y) - 1;
                        self.boards[orientation_index].cols[x] |=
                            mask_drop_from_y - mask_unreachable;
                        col &= (col | (col - 1)) + 1;
                    }
                }
            }
        }

        // From up to down, update the reachable position.
        // It will be updated for each column and repeats the moving down one row until it stops changing.
        #[inline]
        fn update_by_moving_softdrop(&mut self, free_piece_boards: &FreePieceBoards) {
            for orientation_index in 0..4 {
                for x in 0..10 {
                    let free = free_piece_boards.boards[orientation_index].cols[x];

                    // If the position one row down is free, mark it as reachable.
                    let mut updating = self.boards[orientation_index].cols[x];
                    updating |= (updating >> 1) & free;

                    // Repeat that until it stops changing.
                    while self.boards[orientation_index].cols[x] != updating {
                        self.boards[orientation_index].cols[x] = updating;
                        updating |= (updating >> 1) & free;
                    }
                }
            }
        }

        // By rotating, update the reachable position.
        fn update_by_rotating(
            &mut self,
            shape: Shape,
            rotation: Rotation,
            free_piece_boards: &FreePieceBoards,
            previous_pre_rotation: &ReachablePieceBoards,
            rotation_system: &impl RotationSystem,
        ) {
            for piece in shape.all_pieces_iter() {
                let from = piece.to_piece_blocks();
                let to = from.rotate(rotation);
                let bl_offset = to.bottom_left - from.bottom_left;

                // Exclude previously checked positions from reachable positions.
                let mut candidates_board = self.boards[from.piece.orientation as usize];
                candidates_board
                    .remove(&previous_pre_rotation.boards[from.piece.orientation as usize]);

                // For each kick, check to see if it can rotate from the candidate position.
                // If it can rotate, remove it from the candidates.
                for kick in rotation_system.iter_kicks(from.piece(), rotation) {
                    // Ends when the candidate positions are nothing.
                    if candidates_board.is_blank() {
                        break;
                    }

                    let offset = kick.offset + bl_offset;

                    let to = to.piece.orientation as usize;
                    let dx = offset.dx;
                    let dy = offset.dy.unsigned_abs() as usize;

                    let (start, end) = if 0 <= dx {
                        (0, 10 - dx as usize)
                    } else {
                        (-dx as usize, 10)
                    };

                    let free_piece_board = &free_piece_boards.boards[to];
                    if 0 <= offset.dy {
                        let forward_op = u64::shl;
                        let backward_op = u64::shr;
                        self.update_by_rotating_for_a_kick(
                            &mut candidates_board,
                            free_piece_board,
                            to,
                            (dx, dy),
                            (start, end),
                            (forward_op, backward_op),
                        );
                    } else {
                        let forward_op = u64::shr;
                        let backward_op = u64::shl;
                        self.update_by_rotating_for_a_kick(
                            &mut candidates_board,
                            free_piece_board,
                            to,
                            (dx, dy),
                            (start, end),
                            (forward_op, backward_op),
                        );
                    }
                }
            }
        }

        // (private common process for `update_by_rotating`. Inline is strongly recommended.)
        #[inline(always)]
        fn update_by_rotating_for_a_kick(
            &mut self,
            candidates_board: &mut ReachablePieceBoard,
            free_piece_board: &FreePieceBoard,
            to: usize,
            (dx, dy): (i32, usize),
            (start, end): (usize, usize),
            (forward_op, backward_op): (impl Fn(u64, usize) -> u64, impl Fn(u64, usize) -> u64),
        ) {
            for x in start..end {
                let tx = (x as i32 + dx) as usize;
                let from = candidates_board.cols[x];
                let free = free_piece_board.cols[tx];
                let fixed = forward_op(from, dy) & free;
                self.boards[to].cols[tx] |= fixed;
                candidates_board.cols[x] &= !backward_op(fixed, dy);
            }
        }

        // Returns true if a flag at orientation and position is on.
        #[inline]
        pub fn can_reach(&self, orientation: Orientation, position: BlPosition) -> bool {
            self.boards[orientation as usize].can_reach(position)
        }

        // Extract lockable positions from the currently free positions.
        #[inline]
        pub fn extract_landed_positions(&mut self, free_piece_boards: &FreePieceBoards) {
            for orientation_index in 0..4 {
                for x in 0..10 {
                    self.boards[orientation_index].cols[x] &=
                        !(free_piece_boards.boards[orientation_index].cols[x] << 1);
                }
            }
        }

        // Extract canonical positions from the currently free positions.
        #[inline]
        pub fn minimize(&mut self, shape: Shape) {
            let mut visited = ReachablePieceBoards::non_reach();
            for piece in shape.all_pieces_iter() {
                let canonical = piece.canonical_or_self();
                self.boards[piece.orientation as usize]
                    .remove(&visited.boards[canonical.orientation as usize]);
                visited.boards[canonical.orientation as usize]
                    .merge(&self.boards[piece.orientation as usize]);
            }
        }

        #[inline]
        pub fn count_ones(&self) -> u32 {
            self.boards
                .iter()
                .map(|board| board.count_ones())
                .sum::<u32>()
        }
    }

    // Generate boards with reachable locations.
    #[inline]
    pub fn gen_reachable_softdrop(
        spawn: &BlPlacement,
        free_piece_boards: &FreePieceBoards,
        rotation_system: &impl RotationSystem,
    ) -> ReachablePieceBoards {
        gen_reachable_softdrop_with_early_stopping(
            spawn,
            free_piece_boards,
            rotation_system,
            move |_| GenerateInstruction::Continue,
        )
    }

    // Generate boards with reachable locations.
    pub fn gen_reachable_softdrop_with_early_stopping(
        spawn: &BlPlacement,
        free_piece_boards: &FreePieceBoards,
        rotation_system: &impl RotationSystem,
        early_stopping: impl Fn(&ReachablePieceBoards) -> GenerateInstruction,
    ) -> ReachablePieceBoards {
        // ==========================================================================================================
        // [NOTE] The characteristics of each operation are as follows. The order of operations is determined accordingly.
        //
        // # moving right/left
        // The number of internal processes is always fixed at 4*9 times, so it is lightweight and stable
        //
        // # moving down (harddrop)
        // Harddrop destinations are calculated directly, so there is no parallelism.
        // so use it early in the search, when there are few bits in the col.

        // # moving down (softdrop)
        // Internal processing repeats until there are no more changes,
        // so use it when current positions are "very limited or almost completed."
        //
        // # rotating cw/ccw
        // For SRS testing, more processing is required than others, so it is used as little as possible.
        // ==========================================================================================================

        assert!(free_piece_boards.is_free(spawn.piece.orientation, spawn.position));

        let mut reachable_piece_boards = ReachablePieceBoards::non_reach();

        // Harddrop moving
        reachable_piece_boards.mark_spawn_and_its_post_rotations(
            spawn,
            2,
            free_piece_boards,
            rotation_system,
        );

        // Preparation: At least cover the positions reachable by harddrop.
        // In the beginning, changes will almost certainly occur.
        // Thus, predefined operations are applied before the loop.
        {
            reachable_piece_boards.update_by_moving_right(free_piece_boards);
            reachable_piece_boards.update_by_moving_left(free_piece_boards);

            reachable_piece_boards.update_by_moving_harddrop(free_piece_boards);
        }

        // Expand the reachable area without using rotation.
        // Rotating process is heavy and should be done as few times as possible.
        loop {
            if early_stopping(&reachable_piece_boards) == GenerateInstruction::Stop {
                return reachable_piece_boards;
            }

            let freeze = reachable_piece_boards;

            reachable_piece_boards.update_by_moving_right(free_piece_boards);
            reachable_piece_boards.update_by_moving_left(free_piece_boards);
            reachable_piece_boards.update_by_moving_softdrop(free_piece_boards);

            if freeze == reachable_piece_boards {
                break;
            }
        }

        // If the rotation does not change the position, then it's complete.
        if rotation_system
            .is_moving_in_rotation(spawn.piece.shape)
            .not()
        {
            return reachable_piece_boards;
        }

        // Expand the reachable area using rotation.
        // If no change occurs after applying all operations, then it's complete.
        let mut freeze = ReachablePieceBoards::non_reach();

        loop {
            if early_stopping(&reachable_piece_boards) == GenerateInstruction::Stop {
                return reachable_piece_boards;
            }

            // These boards have positions in the previous pre-rotation.
            // These boards is used to cut positions that have already been checked rotation.
            let previous_pre_rotation = freeze;

            // Save boards before operations.
            freeze = reachable_piece_boards;

            // First, it starts with rotation operations, as it does not change except for rotation already.
            // Positions that have already been checked are skipped in the calculation.
            //
            // Precisely, the previous positions of each rotation should be recorded.
            // However, copying also takes time, so it approximates by the board at the start of the last loop (before the last rotation).
            reachable_piece_boards.update_by_rotating(
                spawn.piece.shape,
                Rotation::Cw,
                free_piece_boards,
                &previous_pre_rotation,
                rotation_system,
            );
            reachable_piece_boards.update_by_rotating(
                spawn.piece.shape,
                Rotation::Ccw,
                free_piece_boards,
                &previous_pre_rotation,
                rotation_system,
            );

            // Apply from down because it's faster when there are fewer changes.
            reachable_piece_boards.update_by_moving_softdrop(free_piece_boards);

            // The side moving operations is needed to assure that no changes occur.
            reachable_piece_boards.update_by_moving_right(free_piece_boards);
            reachable_piece_boards.update_by_moving_left(free_piece_boards);

            if freeze == reachable_piece_boards {
                break;
            }
        }

        reachable_piece_boards
    }

    // Generate boards with reachable locations without down move.
    // Targets move that can be moved by rotating at the spawn position, then moving left/right, then hard drop.
    pub fn gen_reachable_harddrop(
        spawn: &BlPlacement,
        free_piece_boards: &FreePieceBoards,
        rotation_system: &impl RotationSystem,
    ) -> ReachablePieceBoards {
        assert!(free_piece_boards.is_free(spawn.piece.orientation, spawn.position));

        let mut reachable_piece_boards = ReachablePieceBoards::non_reach();
        reachable_piece_boards.mark_spawn_and_its_post_rotations(
            spawn,
            2,
            free_piece_boards,
            rotation_system,
        );

        // Left and Right
        reachable_piece_boards.update_by_moving_right(free_piece_boards);
        reachable_piece_boards.update_by_moving_left(free_piece_boards);

        // Harddrop
        reachable_piece_boards.update_by_moving_harddrop(free_piece_boards);

        reachable_piece_boards
    }

    #[derive(Debug)]
    pub struct Moves {
        pub spawn: BlPlacement,
        pub reachable_piece_boards: ReachablePieceBoards,
    }

    impl Moves {
        #[inline]
        pub fn len(&self) -> usize {
            self.reachable_piece_boards.count_ones() as usize
        }

        #[inline]
        pub fn vec(&self) -> Vec<BlPlacement> {
            self.vec_with_capacity(self.len())
        }

        /// `capacity` is a hint and does not affect the result.
        pub fn vec_with_capacity(&self, capacity: usize) -> Vec<BlPlacement> {
            let mut out = Vec::<BlPlacement>::with_capacity(capacity);

            for piece in self.spawn.piece.shape.all_pieces_iter() {
                let board = &self.reachable_piece_boards.boards[piece.orientation as usize];
                for lx in 0..10 {
                    let mut col = board.cols[lx];
                    while 0 < col {
                        let by = col.trailing_zeros();
                        out.push(piece.with(bl(lx as i32, by as i32)));
                        col -= 1u64 << by;
                    }
                }
            }

            out
        }
    }

    pub(crate) fn all_moves_softdrop(
        rotation_system: &impl RotationSystem,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> Moves {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        let mut reachable_piece_boards =
            gen_reachable_softdrop(&spawn, &free_piece_boards, rotation_system);
        reachable_piece_boards.extract_landed_positions(&free_piece_boards);

        Moves {
            spawn,
            reachable_piece_boards,
        }
    }

    pub(crate) fn minimized_moves_softdrop(
        rotation_system: &impl RotationSystem,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> Moves {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        let mut reachable_piece_boards =
            gen_reachable_softdrop(&spawn, &free_piece_boards, rotation_system);
        reachable_piece_boards.extract_landed_positions(&free_piece_boards);
        reachable_piece_boards.minimize(spawn.piece.shape);

        Moves {
            spawn,
            reachable_piece_boards,
        }
    }

    pub(crate) fn can_reach_softdrop(
        rotation_system: &impl RotationSystem,
        goal: BlPlacement,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> bool {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        let orientations = goal.piece.orientations_having_same_form();

        let can_reach =
            |reachable_piece_boards: &ReachablePieceBoards, goal: BlPlacement| -> bool {
                orientations.iter().any(|&orientation| {
                    reachable_piece_boards.can_reach(orientation, goal.position)
                })
            };

        let reachable_piece_boards = gen_reachable_softdrop_with_early_stopping(
            &spawn,
            &free_piece_boards,
            rotation_system,
            |reachable_piece_boards| {
                if can_reach(reachable_piece_boards, goal) {
                    GenerateInstruction::Stop
                } else {
                    GenerateInstruction::Continue
                }
            },
        );

        can_reach(&reachable_piece_boards, goal)
    }

    pub(crate) fn can_reach_strictly_softdrop(
        rotation_system: &impl RotationSystem,
        goal: BlPlacement,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> bool {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        fn can_reach(reachable_piece_boards: &ReachablePieceBoards, goal: BlPlacement) -> bool {
            reachable_piece_boards.can_reach(goal.orientation(), goal.position)
        }

        let reachable_piece_boards = gen_reachable_softdrop_with_early_stopping(
            &spawn,
            &free_piece_boards,
            rotation_system,
            |reachable_piece_boards| {
                if can_reach(reachable_piece_boards, goal) {
                    GenerateInstruction::Stop
                } else {
                    GenerateInstruction::Continue
                }
            },
        );

        can_reach(&reachable_piece_boards, goal)
    }

    pub(crate) fn all_moves_harddrop(
        rotation_system: &impl RotationSystem,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> Moves {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        let mut reachable_piece_boards =
            gen_reachable_harddrop(&spawn, &free_piece_boards, rotation_system);
        reachable_piece_boards.extract_landed_positions(&free_piece_boards);

        Moves {
            spawn,
            reachable_piece_boards,
        }
    }

    pub(crate) fn minimized_moves_harddrop(
        rotation_system: &impl RotationSystem,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> Moves {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        let mut reachable_piece_boards =
            gen_reachable_harddrop(&spawn, &free_piece_boards, rotation_system);
        reachable_piece_boards.extract_landed_positions(&free_piece_boards);
        reachable_piece_boards.minimize(spawn.piece.shape);

        Moves {
            spawn,
            reachable_piece_boards,
        }
    }

    pub(crate) fn can_reach_harddrop(
        rotation_system: &impl RotationSystem,
        goal: BlPlacement,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> bool {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        let reachable_piece_boards =
            gen_reachable_harddrop(&spawn, &free_piece_boards, rotation_system);

        goal.piece
            .orientations_having_same_form()
            .iter()
            .any(|&orientation| reachable_piece_boards.can_reach(orientation, goal.position))
    }

    pub(crate) fn can_reach_strictly_harddrop(
        rotation_system: &impl RotationSystem,
        goal: BlPlacement,
        board: &Board<u64>,
        spawn: BlPlacement,
    ) -> bool {
        let free_board = FreeBoard::from(board);
        let free_piece_boards = FreePieceBoards::new_according_to(spawn.piece.shape, &free_board);

        let reachable_piece_boards =
            gen_reachable_harddrop(&spawn, &free_piece_boards, rotation_system);

        reachable_piece_boards.can_reach(goal.orientation(), goal.position)
    }
}

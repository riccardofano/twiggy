CREATE TRIGGER update_peak_floor_elo UPDATE OF elo_rank ON RPGCharacter
  BEGIN
    UPDATE RPGCharacter SET
        floor_elo = MIN(old.floor_elo, new.elo_rank),
        peak_elo = MAX(old.peak_elo, new.elo_rank)
    WHERE user_id = old.user_id;
  END;

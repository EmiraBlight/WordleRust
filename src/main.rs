use rand::seq::SliceRandom;
use rand::thread_rng;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::read_to_string;
use std::time::Instant;
#[derive(Clone)]
struct Guess {
    word: Box<str>,
    score: f32,
}

impl Guess {
    /*
     * This function will take a vector of all possible words that are left in the array and score the guess.
     * This will change the value of self.score to reflect the bits gained by guessing this particular number.
     * Once this is called it will be thrown into a max heap of size 5 for selection since we need the top 5 best guesses.
     */
    #[allow(dead_code)]
    #[allow(unused_variables)]
    fn score(&mut self, possible_words: &[Guess]) {
        let outcomes = Hint::all_possible(&self.word);
        self.score = 0.0;
        let mut bits: f32 = 0.0;
        for possible in outcomes {
            let px =
                get_remaining(possible_words, &possible) as f32 / (possible_words.len()) as f32;
            if px > 0.0 {
                bits += px * -px.log2();
            }
        }
        self.score = bits;
    }

    fn new(word: &str) -> Self {
        return Guess {
            word: word.into(),
            score: 0.0,
        };
    }
}

impl Ord for Guess {
    fn cmp(&self, other: &Self) -> Ordering {
        // Note: reversed to make BinaryHeap a max-heap
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for Guess {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(other.cmp(self))
    }
}
impl PartialEq for Guess {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}
impl Eq for Guess {}

/*
 * This struct will represent the internal game state. This will keep track of the valid guesses (all the 13k words),
 * the valid words (words not eliminated),
 * and the top 5 guesses given our entropy implamentation
 */

struct Game {
    valid_words: Vec<Guess>,
    valid_guesses: Vec<Guess>,
    top_five_guesses: BinaryHeap<Guess>,
}

impl Game {
    fn new(words: Vec<&str>) -> Self {
        let mut tmp = Game {
            valid_words: Vec::new(),
            valid_guesses: Vec::new(),
            top_five_guesses: BinaryHeap::new(),
        };

        for i in words {
            tmp.valid_words.push(Guess::new(i));
            tmp.valid_guesses.push(Guess::new(i));
        }

        return tmp;
    }
    #[allow(dead_code)]
    fn eliminate_possibles(&mut self, hint_obj: &Hint) {
        let mut index = 0;
        for i in &hint_obj.colors {
            match i {
                Colors::Green => {
                    self.valid_words.retain(|word| {
                        word.word.chars().nth(index) == hint_obj.word.chars().nth(index)
                        //filters out all of the words that have the letter at the same place
                    });
                }
                Colors::Yellow => self.valid_words.retain(|word| {
                    word.word
                        .contains(hint_obj.word.chars().nth(index).unwrap())
                    //filters out all words that do not have this letter in the word
                }),
                Colors::Grey => {
                    let mut counter = 0;
                    for j in 0..5 {
                        match *hint_obj.colors.get(j).unwrap() {
                            Colors::Green => {
                                if hint_obj.word.chars().nth(j) == hint_obj.word.chars().nth(index)
                                {
                                    counter += 1
                                }
                            }
                            Colors::Yellow => {
                                if hint_obj.word.chars().nth(j) == hint_obj.word.chars().nth(index)
                                {
                                    counter += 1
                                }
                            }
                            Colors::Grey => counter += 0,
                        };
                        self.valid_words.retain(|word| {
                            word.word
                                .chars()
                                .filter(|&c| c == hint_obj.word.chars().nth(index).unwrap())
                                .count()
                                == counter
                        })
                    }
                }
            };
            index += 1;
        }
    }
    fn simulate(&mut self, hints: &Vec<Hint>) {
        for hint in hints {
            self.eliminate_possibles(hint);
        }

        let sample_size = 250.max(self.valid_words.len() / 5);

        // Sample if too large
        let to_score: Vec<_> = if self.valid_words.len() > sample_size {
            let mut rng = thread_rng();
            self.valid_words
                .choose_multiple(&mut rng, sample_size)
                .cloned()
                .collect()
        } else {
            self.valid_words.clone()
        };

        let scored_words: Vec<Guess> = self
            .valid_words
            .clone()
            .into_par_iter()
            .map(|mut guess| {
                guess.score(&to_score);
                guess
            })
            .collect();

        self.top_five_guesses.clear();
        for guess in scored_words {
            self.top_five_guesses.push(guess);
            if self.top_five_guesses.len() > 5 {
                self.top_five_guesses.pop(); // Remove lowest score
            }
        }

        println!("TOP 5 ____");
        while self.top_five_guesses.len() > 0 {
            let tmp = self.top_five_guesses.pop().unwrap();
            println!("{}:{}", tmp.word, tmp.score);
        }
    }
}

/*
 * stores information about hints given to player
 */

enum Colors {
    Green,
    Grey,
    Yellow,
}

struct Hint {
    word: Box<str>,
    colors: Vec<Colors>,
}

impl Hint {
    fn new(word: &str, colors: Vec<char>) -> Hint {
        assert_eq!(word.len(), 5);
        assert_eq!(colors.len(), 5);
        let mut tmp = Hint {
            word: word.into(),
            colors: Vec::new(),
        };
        for i in colors {
            if i != 'y' && i != 'g' && i != 'G' {
                panic!("Invalid hint!")
            }
            match i {
                'y' => tmp.colors.push(Colors::Yellow),
                'g' => tmp.colors.push(Colors::Grey),
                'G' => tmp.colors.push(Colors::Green),
                _ => (), //not actually possible but rust wants to catch all cases
            };
        }
        tmp
    }
    fn all_possible(word: &str) -> Vec<Hint> {
        let mut results = Vec::new();
        let options = ['y', 'g', 'G'];

        // Generate all 3^5 = 243 combinations
        for a in &options {
            for b in &options {
                for c in &options {
                    for d in &options {
                        for e in &options {
                            let colors = vec![*a, *b, *c, *d, *e];
                            results.push(Hint::new(word, colors));
                        }
                    }
                }
            }
        }

        results
    }
}
#[allow(dead_code)]
fn get_remaining(given: &[Guess], hint_obj: &Hint) -> usize {
    given
        .iter()
        .filter(|w| is_possible_match(w, hint_obj))
        .count()
}

fn is_possible_match(word: &Guess, hint: &Hint) -> bool {
    let word_chars: Vec<char> = word.word.chars().collect();
    let hint_chars: Vec<char> = hint.word.chars().collect();

    for (i, color) in hint.colors.iter().enumerate() {
        let c = hint_chars[i];
        match color {
            Colors::Green => {
                if word_chars[i] != c {
                    return false;
                }
            }
            Colors::Yellow => {
                if !word_chars.contains(&c) || word_chars[i] == c {
                    return false;
                }
            }
            Colors::Grey => {
                let mut count = 0;
                for j in 0..5 {
                    match *hint.colors.get(j).unwrap() {
                        Colors::Green => {
                            if word_chars[j] == hint_chars[i] {
                                count += 1;
                            }
                        }
                        Colors::Yellow => {
                            if word_chars[j] == hint_chars[i] {
                                count += 1;
                            }
                        }
                        Colors::Grey => (),
                    }
                    if count
                        != word
                            .word
                            .chars()
                            .filter(|&c| c == word.word.chars().nth(i).unwrap())
                            .count()
                    {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn main() {
    unit_tests();
}

fn unit_tests() {
    let mut guesses = Vec::new();
    let read = read_to_string("words.txt").unwrap();
    for line in read.lines() {
        guesses.push(line);
    }
    let first_hint: Hint = Hint::new("tares", vec!['G', 'g', 'g', 'g', 'g']);
    let second_hint: Hint = Hint::new("titty", vec!['G', 'G', 'g', 'g', 'G']);
    let mut new_game = Game::new(guesses);
    let now = Instant::now();
    new_game.simulate(&vec![first_hint]);
    let elapsed_time = now.elapsed();
    println!("ran in {} ms", elapsed_time.as_millis());
    println!("ALL TESTS PASSED!");
}

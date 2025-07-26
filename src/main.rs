use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use openssl::ssl::{SslAcceptor, SslMethod};
use rand::seq::SliceRandom;
use rand::thread_rng;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::fs::File;
use std::io::Read;
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
    fn score(&mut self, possible_words: &[Guess]) {
        let outcomes = Hint::all_possible(&self.word);
        let raw_text = read_to_string("frequency.json").expect("Failed to read freq file");

        let re = Regex::new(r#"(?P<key>\b[a-zA-Z_][a-zA-Z0-9_]*\b)\s*:"#).unwrap();
        let proper_json = re.replace_all(&raw_text, r#""$key":""#);
        //date I got was not in json so I had to format it with a regex

        let freq: HashMap<String, f64> =
            serde_json::from_str(&proper_json).expect("failed to parse json");

        self.score = 0.0;
        let mut bits: f32 = 0.0;
        for possible in outcomes {
            let px =
                get_remaining(possible_words, &possible) as f32 / (possible_words.len()) as f32;
            if px > 0.0 {
                bits += px
                    * -px.log2()
                    * (*freq.get(&possible.word.to_string()).unwrap_or(&(0 as f64)) as f32);
                //some words arent in there, so I will assume if they are not then there score does not matter.
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

        let mut first = Guess::new("tares");
        first.score = 5.0;
        let mut second = Guess::new("lares");
        second.score = 4.0;
        let mut third = Guess::new("rales");
        third.score = 3.0;
        let mut fouth = Guess::new("rates");
        fouth.score = 2.0;
        let mut fifth = Guess::new("teras");
        fifth.score = 1.0;

        tmp.top_five_guesses.push(first);
        tmp.top_five_guesses.push(second);
        tmp.top_five_guesses.push(third);
        tmp.top_five_guesses.push(fouth);
        tmp.top_five_guesses.push(fifth);

        /*
         * due to time complexity I am hard coding in the first 5 words. Since
         * the first guess starts with litrerally 0 information gain
         * every call to this function will aways return the same information
         * scanning all 14k possible words without filtering at least 1 hint
         * is just too much calculating to make this back end feasable
         */

        return tmp;
    }
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
                        .contains(hint_obj.word.chars().nth(index).unwrap())// <- filters out all words that do not have this letter in the word
                        && word.word.chars().nth(index) != hint_obj.word.chars().nth(index) // <-Removes all words with char at index since they cant be there
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
                    }
                    self.valid_words.retain(|word| {
                        word.word
                            .chars()
                            .filter(|&c| c == hint_obj.word.chars().nth(index).unwrap())
                            .count()
                            == counter
                    })
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

        /*
         * since this program running on a backend, the CPU will not be very strong, even with running most of this function in parallel.
         * I cant afford to calculate EVERY possible word and get the exact entropy, it will simply take too long.
         * As a compromise I am limiting the data I collect entropy averages from to a max sample size of 20% the distrobution or 250 words. Whichever is larger.
         * If the total number of words left is less than 250, then the whole words list will be used. As a result, the further into the game we are and the more information
         * we have collected, the more accurate the results likely will be.
         */

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
        //wipe the last top 5 words
        for guess in scored_words {
            self.top_five_guesses.push(guess);
            if self.top_five_guesses.len() > 5 {
                self.top_five_guesses.pop(); // Remove lowest score
            }
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
                _ => (),
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

#[derive(Deserialize)]
struct HintInput {
    word: String,
    hint: String,
}

#[derive(Serialize)]
struct GuessOutput {
    guesses: Vec<String>,
}
use openssl::pkey::PKey;
use openssl::x509::X509;
use std::fs::read_to_string;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    
    let mut cert_file = File::open("/etc/letsencrypt/live/srv915664.hstgr.cloud/cert.pem")?;
    let mut key_file = File::open("/etc/letsencrypt/live/srv915664.hstgr.cloud/privkey.pem")?;
    let mut cert = Vec::new();
    let mut key = Vec::new();
    cert_file.read_to_end(&mut cert)?;
    key_file.read_to_end(&mut key)?;


    let cert = X509::from_pem(&cert).unwrap();
    let key = PKey::private_key_from_pem(&key).unwrap();

    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder.set_certificate(&cert).unwrap();
    builder.set_private_key(&key).unwrap();

    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();
        App::new()
            .wrap(cors)
            .route("/best_guesses", web::post().to(best_guesses))
    })
    .bind_openssl("0.0.0.0:443", builder)?
    .run()
    .await
}
async fn best_guesses(input: web::Json<Vec<HintInput>>) -> impl Responder {
    let words_text = read_to_string("words.txt").unwrap();
    let guesses: Vec<&str> = words_text.lines().collect();

    let mut hint_structs = Vec::new();
    for hint in input.into_inner() {
        let chars = hint.hint.chars().collect();
        hint_structs.push(Hint::new(&hint.word, chars));
    }

    let mut game = Game::new(guesses);
    if !hint_structs.is_empty() {
        game.simulate(&hint_structs);
    }

    let mut output = Vec::new();
    while let Some(guess) = game.top_five_guesses.pop() {
        output.push(guess.word.to_string()); // Convert Box<str> to String
    }

    HttpResponse::Ok().json(GuessOutput { guesses: output })
}

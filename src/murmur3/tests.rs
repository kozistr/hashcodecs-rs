mod incremental;
mod reference;
mod selection;

fn x64_words_as_u128(words: [u64; 2]) -> u128 {
    (words[0] as u128) | ((words[1] as u128) << 64)
}

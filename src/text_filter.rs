use regex::Regex;
use crate::dictionary::{DictEntry, apply_dictionary};

pub fn clean_text(text: &str, dictionary: &[DictEntry], max_len: u32, read_more_text: &str) -> String {
    // 1. ユーザー辞書適用
    let mut result = apply_dictionary(text, dictionary);

    // 2. URL除去
    let url_re = Regex::new(r"https?://[^\s]+").unwrap();
    result = url_re.replace_all(&result, "URL省略").into_owned();

    // 3. 草（wwww等）変換
    let www_re = Regex::new(r"[wｗ]{3,}").unwrap();
    result = www_re.replace_all(&result, "わらわら").into_owned();

    // 4. 連続感嘆符・疑問符
    let excl_re = Regex::new(r"[！!]{3,}").unwrap();
    result = excl_re.replace_all(&result, "！！").into_owned();
    let ques_re = Regex::new(r"[？?]{3,}").unwrap();
    result = ques_re.replace_all(&result, "？？").into_owned();

    // 5. 文字数制限
    if result.chars().count() > max_len as usize {
        result = result.chars().take(max_len as usize).collect::<String>() + " " + read_more_text;
    }

    result.trim().to_string()
}

fn main() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image_page.html");
    let s = std::fs::read_to_string(path).unwrap();
    match jhentai_core::parser::image_page_image(&s) {
        Ok(img) => {
            println!("url = {}", img.url);
            println!("orig = {:?}", img.original_image_url);
            println!("hash = {:?}", img.image_hash);
            println!("reload = {:?}", img.reload_key);
            println!("w/h = {:?}/{:?}", img.width, img.height);
        }
        Err(e) => println!("ERR {e}"),
    }
    println!("gallery = {:?}", jhentai_core::parser::image_page_gallery_url(&s));
}

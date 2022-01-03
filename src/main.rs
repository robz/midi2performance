use midly::Smf;
use std::{env, fs, io::Error, path::Path};
use tch::Tensor;

mod lib;

fn convert_directory_recursively(input_path: &str, output_path: &str) -> Result<(), Error> {
    if !Path::new(output_path).is_dir() {
        fs::create_dir_all(&output_path).expect(&format!(
            "could not create output directory '{}'",
            output_path
        ));
    }
    for entry in fs::read_dir(&input_path)
        .expect(&format!("could not read input directory '{}'", input_path))
    {
        let path = entry?.path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if path.metadata()?.is_dir() {
            println!("processing {}...", name);
            let output_subdir = format!("{}/{}", output_path, name);
            convert_directory_recursively(path.to_str().unwrap(), &output_subdir)?;
            continue;
        }
        let data = fs::read(&path).expect(&format!("Could not read file {:?}", path));
        let mut smf = match Smf::parse(&data) {
            Ok(smf) => smf,
            Err(error) => {
                println!(
                    "Failed to parse file {:?} due to midly error: {}",
                    path, error
                );
                continue;
            }
        };
        let events: Vec<i16> = lib::midi_to_events(&mut smf)
            .into_iter()
            .map(|x| lib::event_to_index(x))
            .collect();
        let output_name = format!("{}/{}.pt", output_path, name);
        println!("{}", output_name);
        Tensor::of_slice(&events)
            .save(output_name)
            .expect("unable to save events to pytorch file");
    }
    Ok(())
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let input_path = &args[1];
    let output_path = &args[2];
    convert_directory_recursively(input_path, output_path)?;
    println!("done!");
    Ok(())
}

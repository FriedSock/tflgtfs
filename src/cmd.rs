use rand;
use ansi_term::Colour::{Green, Yellow, Red, White, Blue};
use rand::distributions::{IndependentSample, Range};
use scoped_threadpool::Pool;
use std::collections::HashSet;
use std::process;
use std::sync::Arc;

use format::{OutputFormat};
use gtfs::*;
use tfl::*;


pub fn fetch_lines(format: OutputFormat, thread_number: u32, sample_size: Option<usize>) {
    let lines = load_lines(DataSource::API, thread_number, sample_size);

    match format {
        OutputFormat::GTFS => transform_gtfs(lines),
        _ => process::exit(0),
    }
}

pub fn transform(format: OutputFormat, thread_number: u32, sample_size: Option<usize>) {
    let lines = load_lines(DataSource::Cache, thread_number, sample_size);

    match format {
        OutputFormat::GTFS => transform_gtfs(lines),
        _ => process::exit(0),
    }
}


fn load_lines(data_source: DataSource, thread_number: u32, sample_size: Option<usize>) -> Vec<Line> {
    let mut pool = Pool::new(thread_number);
    let client = Arc::new(Client::new());

    let mut lines = match data_source {
        DataSource::Cache => client.get_cached_lines(),
        DataSource::API   => client.get_lines(),
    };

    if let Some(n) = sample_size {
        let limit = lines.len();

        if n <= limit {
            let between = Range::new(0usize, limit);
            let mut rng = rand::thread_rng();
            let seed = between.ind_sample(&mut rng);
            let (r, s) = if (seed + n) > limit {
                             ((limit - seed), limit)
                         } else {
                             (seed, (seed + n))
                         };

            println!("Sample: {:?}", (r, s));

            lines = lines[r .. s].to_vec();
        }
    }

    pool.scoped(|scope| {
        for line in &mut lines {
            let client = client.clone();
            scope.execute(move || {
                line.inbound_sequence = client.get_sequence(&line.id, "inbound");
                line.outbound_sequence = client.get_sequence(&line.id, "outbound");
                line.stops = Some(client.get_stops(&line.id));
                for route_section in &mut line.routeSections {
                    println!("Getting Timetable for Line: {}, Route Section: {} ...", line.name, route_section.name);
                    route_section.timetable = client.get_timetable(&line.id, &route_section.originator, &route_section.destination);
                }
            });
        }
    });

    lines
}

fn transform_gtfs(lines: Vec<Line>) {
    let mut line_count = 0;
    let mut line_ids: HashSet<String> = HashSet::new();
    let mut route_section_count = 0;
    let mut route_section_ids: HashSet<String> = HashSet::new();
    let mut schedule_names: HashSet<String> = HashSet::new();

    for line in &lines {
        let is_duplicated = match line_ids.contains(&line.id) {
            true => Red.paint("yes"),
            false => Green.paint("no"),
        };

        println!("{}; Duplicate: {}", line, is_duplicated);

        for route_section in &line.routeSections {
            let has_timetable = match route_section.timetable {
                Some(ref timetable) => {
                    let names = timetable.schedule_names();
                    schedule_names = schedule_names.union(&names).cloned().collect::<HashSet<String>>();
                    names.is_empty()
                },
                None => false,
            };

            let id = route_section_id(&line, &route_section);
            println!("\t{}, Has Timetable: {}, Duplicate: {}", id, has_timetable, route_section_ids.contains(&id));
            route_section_ids.insert(id.clone());
            route_section_count += 1;
        }
        line_count += 1;
        line_ids.insert(line.id.clone());
    }

    if lines.is_empty() {
        println!("No lines found in the cache, try fetching some data first");
        process::exit(0);
    }

    println!("Duplicate Lines: {}, Duplicate Route Sections: {}", line_count - line_ids.len(), route_section_count-route_section_ids.len());

    println!("Schedule Names:");
    for schedule_name in &schedule_names {
        println!("\t{}", schedule_name);
    }

    // Generate CSV files from fetched data
    write_gtfs(&lines);
}

use std::time::Duration;

use rand::distributions::WeightedIndex;
use rand::prelude::*;

use rstream::config::EnvironmentConfig;
use rstream::environment::StreamEnvironment;
use rstream::operator::source::IteratorSource;
use rstream::operator::{EventTimeWindow, Timestamp};

fn random_topic(t: u64) -> (Timestamp, String) {
    let mut rng = rand::thread_rng();

    let topics = [
        "#photo",
        "#picoftheday",
        "#instalike",
        "#photooftheday",
        "#instadaily",
        "#photography",
        "#likeforlikes",
        "#love",
        "#instagood",
    ];
    let dist = (0..topics.len()).map(|i| 0.5f64.powf(i as f64));
    let dist = WeightedIndex::new(dist).unwrap();

    std::thread::sleep(Duration::from_millis(10));
    let tag = topics[dist.sample(&mut rng)].to_string();
    (Timestamp::from_millis(10 * t), tag)
}

fn main() {
    let items = (0..10000).map(random_topic);
    let win_size = 1000;
    let win_step = 500;
    let k = 4;

    let (config, _args) = EnvironmentConfig::from_args();
    let mut env = StreamEnvironment::new(config);
    env.spawn_remote_workers();

    let source = IteratorSource::new(items);
    env.stream(source)
        // add a timestamp for each item (using the one generated by the source) and add a watermark
        // every 10 items
        .add_timestamps(|(ts, _)| *ts, {
            let mut count = 0;
            move |_, &ts| {
                count += 1;
                if count % 10 == 0 {
                    Some(ts)
                } else {
                    None
                }
            }
        })
        // forget about the timestamp, it's already attached to the messages
        .map(|(_ts, w)| w)
        // count each word separately
        .group_by(|w| w.clone())
        .window(EventTimeWindow::sliding(
            Duration::from_millis(win_size),
            Duration::from_millis(win_step),
        ))
        // count how many times each word appears in the window
        .map(|w| w.len())
        .unkey()
        // bottleneck for computing the ranking of each window
        .group_by(|_| ())
        // this window has the same alignment of the previous one, so it will contain the same items
        .window(EventTimeWindow::tumbling(Duration::from_millis(win_step)))
        .map(move |w| {
            // find the k most frequent words for each window
            let mut words = w.cloned().collect::<Vec<(String, usize)>>();
            words.sort_by_key(|(_w, c)| -(*c as i64));
            words.resize_with(k.min(words.len()), Default::default);
            words
        })
        .unkey()
        .for_each(|(_, win)| {
            println!("New window");
            for (word, count) in win {
                println!("- {} ({})", word, count);
            }
            println!();
        });
    env.execute();
}

use std::time::{Duration, Instant, SystemTime};

use rand::prelude::*;

use noir::operator::source::ParallelIteratorSource;
use noir::operator::window::EventTimeWindow;
use noir::EnvironmentConfig;
use noir::StreamEnvironment;

const TOPICS: [&str; 50] = [
    "#love",
    "#instagood",
    "#fashion",
    "#photooftheday",
    "#beautiful",
    "#art",
    "#photography",
    "#happy",
    "#picoftheday",
    "#cute",
    "#follow",
    "#tbt",
    "#followme",
    "#nature",
    "#like",
    "#travel",
    "#instagram",
    "#style",
    "#repost",
    "#summer",
    "#instadaily",
    "#selfie",
    "#me",
    "#friends",
    "#fitness",
    "#girl",
    "#food",
    "#fun",
    "#beauty",
    "#instalike",
    "#smile",
    "#family",
    "#photo",
    "#life",
    "#likeforlike",
    "#music",
    "#ootd",
    "#follow",
    "#makeup",
    "#amazing",
    "#igers",
    "#nofilter",
    "#dog",
    "#model",
    "#sunset",
    "#beach",
    "#instamood",
    "#foodporn",
    "#motivation",
    "#followforfollow",
];
const PROB: f64 = 0.1;

fn random_topic() -> String {
    let mut rng = rand::thread_rng();

    for topic in TOPICS {
        if rng.gen::<f64>() < PROB {
            return topic.to_string();
        }
    }
    TOPICS[0].to_string()
}

#[derive(Clone)]
struct ThroughputTester {
    name: String,
    count: usize,
    limit: usize,
    last: Instant,
    start: Instant,
    total: usize,
}

impl ThroughputTester {
    fn new(name: String, limit: usize) -> Self {
        Self {
            name,
            count: 0,
            limit,
            last: Instant::now(),
            start: Instant::now(),
            total: 0,
        }
    }

    fn add(&mut self) {
        self.count += 1;
        self.total += 1;
        if self.count > self.limit {
            let elapsed = self.last.elapsed();
            eprintln!(
                "{}: {:10.2}/s @ {}",
                self.name,
                self.count as f64 / elapsed.as_secs_f64(),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            self.count = 0;
            self.last = Instant::now();
        }
    }
}

impl Drop for ThroughputTester {
    fn drop(&mut self) {
        eprintln!(
            "(done) {}: {:10.2}/s (total {})",
            self.name,
            self.total as f64 / self.start.elapsed().as_secs_f64(),
            self.total,
        );
    }
}

struct TopicSource {
    tester: ThroughputTester,
    start: Instant,
    id: u64,
    num_replicas: u64,
    num_gen: u64,
}

impl TopicSource {
    fn new(id: u64, num_replicas: u64) -> Self {
        Self {
            tester: ThroughputTester::new(format!("source{}", id), 50_000),
            start: Instant::now(),
            id,
            num_replicas,
            num_gen: 0,
        }
    }
}

impl Iterator for TopicSource {
    type Item = (Duration, String);

    fn next(&mut self) -> Option<Self::Item> {
        if self.start.elapsed().as_secs() > 10 {
            return None;
        }
        let topic = random_topic();
        let ts = Duration::from_millis(self.num_gen * self.num_replicas + self.id);
        self.num_gen += 1;
        self.tester.add();

        Some((ts, topic))
    }
}

fn main() {
    let win_size = 1000;
    let win_step = 500;
    let k = 4;

    let (config, _args) = EnvironmentConfig::from_args();
    let mut env = StreamEnvironment::new(config);
    env.spawn_remote_workers();

    let source = ParallelIteratorSource::new(|id, num_replicas| {
        TopicSource::new(id as u64, num_replicas as u64)
    });
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
        // this window has the same alignment of the previous one, so it will contain the same items
        .window_all(EventTimeWindow::tumbling(Duration::from_millis(win_step)))
        .map(move |w| {
            // find the k most frequent words for each window
            let mut words = w.cloned().collect::<Vec<(String, usize)>>();
            words.sort_by_key(|(_w, c)| -(*c as i64));
            words.resize_with(k.min(words.len()), Default::default);
            words
        })
        .for_each({
            let mut tester = ThroughputTester::new("sink".into(), 100);
            move |_win| {
                tester.add();
            }
        });
    env.execute();
}

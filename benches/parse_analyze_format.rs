//! Criterion benchmarks for parse(), analyze(), and format().

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use leekscript_rs::formatter::FormatterOptions;
use leekscript_rs::{analyze, format, parse};

const SMALL: &str = r#"
var x = 1;
return x + 2;
"#;

const MEDIUM: &str = r#"
function add(integer a, integer b) -> integer {
    return a + b;
}
function mul(integer a, integer b) -> integer {
    return a * b;
}
integer x = add(1, 2);
integer y = mul(x, 3);
return add(x, y);
"#;

const LARGE: &str = r#"
class Cell {
    public integer id;
    public integer x;
    public integer y;
    public boolean isWall;
    private constructor(integer id) {
        this.id = id;
    }
    public static Cell create(integer pos) {
        var cell = Cell.getCell(pos);
        if (cell == null) {
            push(Cell.cells, new Cell(pos));
        }
        return cell;
    }
    static Array<Cell> cells;
    static Cell? getCell(integer id) {
        return Cell.cells[id];
    }
}
class Entity {
    public integer id;
    public Cell cell;
    private constructor(integer id) {
        this.id = id;
    }
    public update() {
        this.cell = Cell.getCell(getCell(this.id));
    }
    public static Entity create(integer id) {
        return new Entity(id);
    }
    static Array<Entity> entities;
    static Entity? getEntity(integer id) {
        return Entity.entities[id];
    }
}
return null;
"#;

fn bench_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse");
    g.bench_function("parse_small", |b| {
        b.iter(|| parse(black_box(SMALL)).unwrap())
    });
    g.bench_function("parse_medium", |b| {
        b.iter(|| parse(black_box(MEDIUM)).unwrap())
    });
    g.bench_function("parse_large", |b| {
        b.iter(|| parse(black_box(LARGE)).unwrap())
    });
    g.finish();
}

fn bench_analyze(c: &mut Criterion) {
    let small_root = parse(SMALL).unwrap().expect("small");
    let medium_root = parse(MEDIUM).unwrap().expect("medium");
    let large_root = parse(LARGE).unwrap().expect("large");

    let mut g = c.benchmark_group("analyze");
    g.bench_function("analyze_small", |b| {
        b.iter(|| analyze(black_box(&small_root)))
    });
    g.bench_function("analyze_medium", |b| {
        b.iter(|| analyze(black_box(&medium_root)))
    });
    g.bench_function("analyze_large", |b| {
        b.iter(|| analyze(black_box(&large_root)))
    });
    g.finish();
}

fn bench_format(c: &mut Criterion) {
    let small_root = parse(SMALL).unwrap().expect("small");
    let medium_root = parse(MEDIUM).unwrap().expect("medium");
    let opts = FormatterOptions::default();

    let mut g = c.benchmark_group("format");
    g.bench_function("format_small", |b| {
        b.iter(|| format(black_box(&small_root), black_box(&opts)))
    });
    g.bench_function("format_medium", |b| {
        b.iter(|| format(black_box(&medium_root), black_box(&opts)))
    });
    g.finish();
}

criterion_group!(benches, bench_parse, bench_analyze, bench_format);
criterion_main!(benches);

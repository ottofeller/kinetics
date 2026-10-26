use super::utils::{replace, CargoFixture};

#[test]
fn unchanged_workspace_keeps_every_cargo_target_fresh() {
    let fixture = CargoFixture::new();
    fixture.assert_rebuilt(&[]);
}

#[test]
fn endpoint_edit_recompiles_only_its_binary() {
    let fixture = CargoFixture::new();
    replace(fixture.project.source.path(), "src/api.rs", "+ 20", "+ 21");
    fixture.assert_rebuilt(&["SampleApiRequest"]);
    fixture.assert_output("SampleApiRequest", "22");
}

#[test]
fn worker_edit_recompiles_only_its_binary() {
    let fixture = CargoFixture::new();
    replace(
        fixture.project.source.path(),
        "src/jobs.rs",
        "+ helper()",
        "+ helper() + 1",
    );
    fixture.assert_rebuilt(&["SampleJobsWorker"]);
    fixture.assert_output("SampleJobsWorker", "12");
}

#[test]
fn private_helper_edit_keeps_the_library_and_other_binaries_fresh() {
    let fixture = CargoFixture::new();
    replace(
        fixture.project.source.path(),
        "src/jobs.rs",
        "    10",
        "    11",
    );
    fixture.assert_rebuilt(&["SampleJobsWorker"]);
    fixture.assert_output("SampleJobsWorker", "12");
}

#[test]
fn handlers_in_the_same_file_recompile_together() {
    let fixture = CargoFixture::new();
    replace(fixture.project.source.path(), "src/pair.rs", "+ 30", "+ 31");
    fixture.assert_rebuilt(&["SamplePairFirst", "SamplePairSecond"]);
    fixture.assert_output("SamplePairFirst", "32");
    fixture.assert_output("SamplePairSecond", "41");
}

#[test]
fn shared_code_edit_recompiles_dependents_but_keeps_other_packages_fresh() {
    let fixture = CargoFixture::new();
    replace(
        fixture.project.source.path(),
        "src/shared.rs",
        "    1",
        "    2",
    );
    fixture.assert_rebuilt(&[
        "sample",
        "SampleApiRequest",
        "SampleJobsWorker",
        "SamplePairFirst",
        "SamplePairSecond",
    ]);
    fixture.assert_output("SampleApiRequest", "22");
    fixture.assert_output("SampleJobsWorker", "12");
    fixture.assert_output("SamplePairFirst", "32");
    fixture.assert_output("SamplePairSecond", "42");
}

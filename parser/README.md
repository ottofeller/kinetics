# Kinetics parser
The crate provides primitives used for kinetics macro parsing, either in macro itself or by CLI. The entities include available function types, their arguments, as well as rules for argument validation.

Most probably if you're not developing the Kinetics project you don't need this crate. In order to deploy within Kinetics project use the crates listed below.

# Downstream crates
## [kinetics-macros](https://crates.io/crates/kinetics-macros)
The macros used to mark your functions as deployment units. The functions then can be deployed with the Kinetics CLI.

## [kinetics](https://crates.io/crates/kinetics)
The core library of the Kinetics project. The crate provides a CLI that can parse all `kinetics-macros` instances in a project, build the marked functions as lambdas, and deploy them.

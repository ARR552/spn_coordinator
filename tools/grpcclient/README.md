

cargo run -- proof-request-details --url https://rpc-production.succinct.xyz --request-id=4e94a6a152d166b9c26faf27e406ead95b60aee02da50294e10a46131fbb9f5f

cargo run -- proof-request-status --url https://rpc-production.succinct.xyz --request-id=4e94a6a152d166b9c26faf27e406ead95b60aee02da50294e10a46131fbb9f5f

cargo run -- get-program --url https://rpc-production.succinct.xyz --vk-hash 003991487ea72a40a1caa7c234b12c0da52fc4ccc748a07f6ebd354bbb54772e

cargo run --release -- verify-proof --proof-url http://localhost:8082/artifacts/Proof/1992a1009306959037ecb8844f03cf00 --vk 4351fc69c6e700272d6c4508143ff04d81662c259cd4d40c0b5c6b50774af3496c1c2000a62e8f269193707238dc696c098a0a05f9395073e8b642687bec4928d506736b4e65604a2df560063a490974e92a0705308eb6413cc4f4390200000000000000070000000000000050726f6772616d1300000000000000010000000e0000000000000000000800000000000400000000000000427974651000000000000000010000000b0000000000000000000100000000000200000000000000070000000000000050726f6772616d00000000000000000400000000000000427974650100000000000000

cargo run -- create-program --url "http://localhost:50051" --private-key 0xe5d76acbffb5be6d87002e2cd5622b6dfe715f73ac60c613f14ba2d3f735c20b --elf-path ../../src/client/elf/aggregation-elf

cargo run -- create-program --url "http://localhost:50051" --private-key 0xe5d76acbffb5be6d87002e2cd5622b6dfe715f73ac60c613f14ba2d3f735c20b --elf-path ../../src/client/elf/celestia-range-elf-embedded

cargo run -- create-program --url "http://localhost:50051" --private-key 0xe5d76acbffb5be6d87002e2cd5622b6dfe715f73ac60c613f14ba2d3f735c20b --elf-path ../../src/client/elf/range-elf-bump

cargo run -- create-program --url "http://localhost:50051" --private-key 0xe5d76acbffb5be6d87002e2cd5622b6dfe715f73ac60c613f14ba2d3f735c20b --elf-path ../../src/client/elf/range-elf-embedded

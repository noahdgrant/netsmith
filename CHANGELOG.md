# Changelog

## [0.1.0](https://github.com/noahdgrant/netsmith/compare/v0.0.1...v0.1.0) (2026-11-08)


### Features

* **check:** a module's rail forwards the current of the part inside it (ADR 0077) ([#217](https://github.com/noahdgrant/netsmith/issues/217)) ([f50873f](https://github.com/noahdgrant/netsmith/commit/f50873fe2de9adef912214a629dbe3e3e631c420))
* **check:** warn at build about a generic that no catalog row can resolve ([#208](https://github.com/noahdgrant/netsmith/issues/208)) ([b929ffa](https://github.com/noahdgrant/netsmith/commit/b929ffa6384a276c6b03e9a70172ac037c5f294b))
* **data:** add the BZT52C39 zener, the SMDJ36A TVS and the BLM31PG601SN1L bead ([#233](https://github.com/noahdgrant/netsmith/issues/233)) ([74c14b0](https://github.com/noahdgrant/netsmith/commit/74c14b0b9d65c624d2fd751f83b543ea6e169167))
* **data:** add the SS3H10 Schottky and three 100 V X7R capacitors from issue 179 ([#216](https://github.com/noahdgrant/netsmith/issues/216)) ([75069f1](https://github.com/noahdgrant/netsmith/commit/75069f11943b46e44bf2a9b01c9a8b610a3aad78)), closes [#179](https://github.com/noahdgrant/netsmith/issues/179)
* **editor:** report the power problems from Rust, in build and in the analysis ([#218](https://github.com/noahdgrant/netsmith/issues/218)) ([058ad05](https://github.com/noahdgrant/netsmith/commit/058ad05117a636682de4322c01032b87fa1083cf)), closes [#187](https://github.com/noahdgrant/netsmith/issues/187)
* **lang:** a dc statement and a level with a test current (ADR 0076, stage 1) ([#223](https://github.com/noahdgrant/netsmith/issues/223)) ([4ffe916](https://github.com/noahdgrant/netsmith/commit/4ffe916ce240d01671ae7f2d6a88d1731d3d8e1f))
* **lang:** declare a catalog's optional columns in the catalog statement (ADR 0078) ([#222](https://github.com/noahdgrant/netsmith/issues/222)) ([2339a6b](https://github.com/noahdgrant/netsmith/commit/2339a6b5eb40ee032cd6cfe8b909a3c42b6224f5)), closes [#178](https://github.com/noahdgrant/netsmith/issues/178)
* **solve:** a DC analysis of the resistive islands (ADR 0076, stage 2a) ([#226](https://github.com/noahdgrant/netsmith/issues/226)) ([7704344](https://github.com/noahdgrant/netsmith/commit/77043444b2e818683f96eb3fd6993eae4922e4e2))
* **solve:** note a picked part that is known to be out of stock ([#202](https://github.com/noahdgrant/netsmith/issues/202)) ([fc68012](https://github.com/noahdgrant/netsmith/commit/fc680126d381e3e294c6a063a1d9b2e9f12a2347))
* **solve:** Ohm bound, optional marker, open-drain levels, track-width floor and the DC analysis ADR ([#207](https://github.com/noahdgrant/netsmith/issues/207)) ([bc7a690](https://github.com/noahdgrant/netsmith/commit/bc7a690e01fbcbbdfe9a4f47e7af16f5ddfb45a4))
* **std:** a pin states its own swing (ADR 0081) ([#230](https://github.com/noahdgrant/netsmith/issues/230)) ([0396387](https://github.com/noahdgrant/netsmith/commit/03963872d3e08ece30744be373014bb380c900ea))
* **std:** a PullDown that states its resistor's current ([#224](https://github.com/noahdgrant/netsmith/issues/224)) ([6257619](https://github.com/noahdgrant/netsmith/commit/625761935aea84a08f259cf1cb325c7c1f31d0af))
* **std:** a Zener states its test current, temperature coefficient and leakage ([#225](https://github.com/noahdgrant/netsmith/issues/225)) ([a455391](https://github.com/noahdgrant/netsmith/commit/a4553913f068658f387d7a2d28232cb5f354dcf3))


### Bug Fixes

* **editor:** a section the person closed does not open again when the project is read again ([#197](https://github.com/noahdgrant/netsmith/issues/197)) ([495fc9f](https://github.com/noahdgrant/netsmith/commit/495fc9f20180aed587d902f25cfd795d2e2aedac))
* **editor:** re-read netsmith.toml when it changes in a running editor ([#210](https://github.com/noahdgrant/netsmith/issues/210)) ([c8d21a4](https://github.com/noahdgrant/netsmith/commit/c8d21a4be4cad463189aab9b62a74a5ffaa756fc))
* **solve:** end a runaway pick search with a message that names the picks ([#205](https://github.com/noahdgrant/netsmith/issues/205)) ([afa1461](https://github.com/noahdgrant/netsmith/commit/afa14616548144b4b7630c3a4d615159cc889c04))
* **solve:** levels stated as fractions of one rail are compared at one supply value ([#231](https://github.com/noahdgrant/netsmith/issues/231)) ([d07f631](https://github.com/noahdgrant/netsmith/commit/d07f63187c2a2ddfbabfc066626e180a6f882958))
* **std:** a levels reference rail is a reference, not a load ([#209](https://github.com/noahdgrant/netsmith/issues/209)) ([3b2ae93](https://github.com/noahdgrant/netsmith/commit/3b2ae9342ea0466c3ba2480a7054aadca1d69ab0))
* **test:** skip every test that needs KiCad when Windows KiCad cannot be started ([#198](https://github.com/noahdgrant/netsmith/issues/198)) ([9e2ed59](https://github.com/noahdgrant/netsmith/commit/9e2ed59fe8fe20c08f1b9d0ba30eb72a0b790d70))
* **test:** the alternatives test relies on catalog_dirs, not a catalogs entry ([#215](https://github.com/noahdgrant/netsmith/issues/215)) ([93d59e2](https://github.com/noahdgrant/netsmith/commit/93d59e2b8c340c1b158a7e0b47586f807e418244))


### Performance

* **solve:** narrow the second pick of a pair by the bounds the first one fixes ([#219](https://github.com/noahdgrant/netsmith/issues/219)) ([83bac21](https://github.com/noahdgrant/netsmith/commit/83bac21f0dd4915fc6d0c3e88ddd67cb987c20d0)), closes [#184](https://github.com/noahdgrant/netsmith/issues/184)
* **solve:** re-solve alternatives only where they can be shown ([#204](https://github.com/noahdgrant/netsmith/issues/204)) ([c747902](https://github.com/noahdgrant/netsmith/commit/c7479027fbb9a9735c9c58fc319e3621e4c77800))


### Documentation

* **adr:** say how ADR 0064's Bus and ADR 0067's Bus drawing relate ([#214](https://github.com/noahdgrant/netsmith/issues/214)) ([fd223c5](https://github.com/noahdgrant/netsmith/commit/fd223c59423a377d90c3ce6a22e358e65e1d0b00))
* **guide:** teach which current statement starts a circle, and its sign ([#213](https://github.com/noahdgrant/netsmith/issues/213)) ([897c401](https://github.com/noahdgrant/netsmith/commit/897c401cd366b543308b9637de3be0b18986ef6e))
* **std:** say what a part in a chain does not see of its rail, with tests ([#211](https://github.com/noahdgrant/netsmith/issues/211)) ([6bc7e98](https://github.com/noahdgrant/netsmith/commit/6bc7e984cf26801ca78121d87a53d1f099c23746))

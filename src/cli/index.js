#!/usr/bin/env node
import { Command } from 'commander';
import { install, start, stop } from './install.js';
import { uninstall } from './uninstall.js';
import { status } from './status.js';
import { vault } from './vault.js';
import { doctor } from './doctor.js';
import { migrateV1 } from './migrate-v1.js';

const program = new Command();
program.name('clipboard-history').description('macOS clipboard history MCP control plane').version('0.2.0-alpha.0');

program.command('install').option('--window-titles', 'capture window titles too').action(install);
program.command('uninstall').option('--keep-data', 'do not remove DB').action(uninstall);
program.command('status').action(status);
program.command('start').action(start);
program.command('stop').action(stop);
program.command('vault').argument('<sub>', 'list | unlock <id>').argument('[id]').action(vault);
program.command('doctor').action(doctor);
program.command('migrate-v1').action(migrateV1);

program.parseAsync(process.argv);

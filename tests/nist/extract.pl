#!/usr/bin/env perl
# extract.pl — Extract individual COBOL test programs from newcob.val
#
# Replaces EXEC85 (which is itself a COBOL program).
# Reads newcob.val and splits it into individual .cob files by module.
#
# Usage: perl extract.pl [newcob.val] [output-dir]
#
# Output structure:
#   output-dir/NC/NC101A.cob
#   output-dir/IC/IC101A.cob
#   output-dir/COPYLIB/ALTL1.cpy
#   ...

use strict;
use warnings;
use File::Path qw(make_path);

my $input  = shift || "newcob.val";
my $outdir = shift || "programs";

open(my $fh, '<', $input) or die "Cannot open $input: $!\n";

my $current_name   = "";
my $current_module = "";
my $current_type   = "";  # COBOL or CLBRY
my $current_fh;
my $program_count  = 0;
my $copylib_count  = 0;
my %module_counts;
my $in_program = 0;

while (my $line = <$fh>) {
    # Detect *HEADER lines
    if ($line =~ /^\*HEADER,(\w+),(\w+)/) {
        my $type = $1;
        my $name = $2;

        # Close previous file
        if ($current_fh) {
            close($current_fh);
            $current_fh = undef;
        }

        $current_type = $type;
        $current_name = $name;

        if ($type eq "CLBRY") {
            # Copy library
            my $dir = "$outdir/COPYLIB";
            make_path($dir) unless -d $dir;
            my $outfile = "$dir/$name.cpy";
            open($current_fh, '>', $outfile) or die "Cannot create $outfile: $!\n";
            $copylib_count++;
            $in_program = 1;
        } elsif ($type eq "COBOL") {
            # Determine module from first 2 characters of name
            $current_module = substr($name, 0, 2);
            my $dir = "$outdir/$current_module";
            make_path($dir) unless -d $dir;
            my $outfile = "$dir/$name.cob";
            open($current_fh, '>', $outfile) or die "Cannot create $outfile: $!\n";
            $program_count++;
            $module_counts{$current_module}++;
            $in_program = 1;
        } else {
            $in_program = 0;
        }

        next;  # Don't write the *HEADER line
    }

    # Skip *END lines and other control lines
    next if $line =~ /^\*/;

    # Write source line to current file (strip trailing padding, keep columns 1-80)
    if ($in_program && $current_fh) {
        # newcob.val has 80-column fixed format lines (padded with spaces)
        # Keep the line as-is for fixed-format compilation
        chomp $line;
        # Truncate to 80 columns (the COBOL source area)
        my $src = substr($line, 0, 80);
        print $current_fh "$src\n";
    }
}

# Close last file
if ($current_fh) {
    close($current_fh);
}
close($fh);

# Summary
print "Extracted $program_count COBOL programs + $copylib_count copy libraries to $outdir/\n";
print "Modules:\n";
for my $mod (sort keys %module_counts) {
    printf "  %-4s: %3d programs\n", $mod, $module_counts{$mod};
}

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
my %seen_programs;
my %seen_copylibs;
my $in_program = 0;

sub parse_header {
    my ($line) = @_;
    chomp $line;
    return unless $line =~ /^\*HEADER,/;
    my @parts = split(/,/, $line);
    return if @parts < 3;
    for my $part (@parts) {
        $part =~ s/^\s+//;
        $part =~ s/\s+$//;
    }

    my $root_name = $parts[2];
    $root_name =~ s/^\s+//;
    $root_name =~ s/\s+$//;
    $root_name =~ s/\s+.*$//;

    my %header = (
        type      => $parts[1],
        root_name => $root_name,
        is_subprg => 0,
    );

    if (@parts >= 5 && $parts[3] eq 'SUBPRG') {
        $header{is_subprg} = 1;
        my $sub_name = $parts[4];
        $sub_name =~ s/^\s+//;
        $sub_name =~ s/\s+$//;
        $sub_name =~ s/\s+.*$//;
        $header{sub_name}  = $sub_name;
    }

    return \%header;
}

while (my $line = <$fh>) {
    # Detect *HEADER lines
    if (my $header = parse_header($line)) {
        my $type = $header->{type};
        my $root_name = $header->{root_name};

        # Close previous file unless this header continues the same
        # top-level COBOL compile unit with a SUBPRG segment.
        if ($current_fh) {
            my $same_cobol_unit =
                $type eq "COBOL"
                && $header->{is_subprg}
                && $current_type eq "COBOL"
                && $current_name eq $root_name;
            if (!$same_cobol_unit) {
                close($current_fh);
                $current_fh = undef;
            }
        }

        $current_type = $type;
        $current_name = $root_name;

        if ($type eq "CLBRY") {
            my $dir = "$outdir/COPYLIB";
            make_path($dir) unless -d $dir;
            my $outfile = "$dir/$root_name.cpy";
            open($current_fh, '>', $outfile) or die "Cannot create $outfile: $!\n";
            if (!$seen_copylibs{$root_name}++) {
                $copylib_count++;
            }
            $in_program = 1;
        } elsif ($type eq "COBOL") {
            $current_module = substr($root_name, 0, 2);
            my $dir = "$outdir/$current_module";
            make_path($dir) unless -d $dir;
            my $outfile = "$dir/$root_name.cob";
            if (!$current_fh) {
                my $mode = $header->{is_subprg} ? '>>' : '>';
                open($current_fh, $mode, $outfile) or die "Cannot create $outfile: $!\n";
            }
            if (!$seen_programs{$root_name}++) {
                $program_count++;
                $module_counts{$current_module}++;
            }
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
